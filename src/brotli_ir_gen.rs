use core::marker::PhantomData;
use core::cmp::min;
use super::probability::{CDF2,CDF16, Speed};
use super::brotli;
pub use super::alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use super::interface::{BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType};

pub use super::cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec};
use super::resizable_byte_buffer::ResizableByteBuffer;
use super::interface;
use super::brotli::BrotliResult;
use super::brotli::enc::encode::{BrotliEncoderStateStruct, BrotliEncoderCompressStream, BrotliEncoderOperation, BrotliEncoderIsFinished};
use super::divans_compressor::write_header;
pub struct BrotliDivansHybridCompressor<SelectedCDF:CDF16,
                            ChosenEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                            AllocU8:Allocator<u8>,
                            AllocU16:Allocator<u16>,
                            AllocU32:Allocator<u32>,
                            AllocI32:Allocator<i32>,
                            AllocCommand:Allocator<super::brotli::enc::command::Command>,
                            AllocCDF2:Allocator<CDF2>,
                            AllocCDF16:Allocator<SelectedCDF>,
                            AllocF64: Allocator<brotli::enc::util::floatX>,
                            AllocFV: Allocator<brotli::enc::vectorization::Mem256f>,
                            AllocHL: Allocator<brotli::enc::histogram::HistogramLiteral>,
                            AllocHC: Allocator<brotli::enc::histogram::HistogramCommand>,
                            AllocHD: Allocator<brotli::enc::histogram::HistogramDistance>,
                            AllocHP: Allocator<brotli::enc::cluster::HistogramPair>,
                            AllocCT: Allocator<brotli::enc::histogram::ContextType>,
                            AllocHT: Allocator<brotli::enc::entropy_encode::HuffmanTree>
     > {
    brotli_encoder: BrotliEncoderStateStruct<AllocU8, AllocU16, AllocU32, AllocI32, AllocCommand>,
    codec: DivansCodec<ChosenEncoder, EncoderSpecialization, SelectedCDF, AllocU8, AllocCDF2, AllocCDF16>,
    header_progress: usize,
    window_size: u8,
    mf64: AllocF64,
    mfv: AllocFV,
    mhl: AllocHL,
    mhc: AllocHC,
    mhd: AllocHD,
    mhp: AllocHP,
    mct: AllocCT,
    mht: AllocHT,
    brotli_data: ResizableByteBuffer<u8, AllocU8>,
    divans_data: ResizableByteBuffer<u8, AllocU8>,
    encoded_byte_offset: usize,
}



impl<SelectedCDF:CDF16,
     ChosenEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocU16:Allocator<u16>,
     AllocU32:Allocator<u32>,
     AllocI32:Allocator<i32>,
     AllocCommand:Allocator<super::brotli::enc::command::Command>,
     AllocCDF2:Allocator<CDF2>,
     AllocCDF16:Allocator<SelectedCDF>,
     AllocF64: Allocator<brotli::enc::util::floatX>,
     AllocFV: Allocator<brotli::enc::vectorization::Mem256f>,
     AllocHL: Allocator<brotli::enc::histogram::HistogramLiteral>,
     AllocHC: Allocator<brotli::enc::histogram::HistogramCommand>,
     AllocHD: Allocator<brotli::enc::histogram::HistogramDistance>,
     AllocHP: Allocator<brotli::enc::cluster::HistogramPair>,
     AllocCT: Allocator<brotli::enc::histogram::ContextType>,
     AllocHT: Allocator<brotli::enc::entropy_encode::HuffmanTree>
     > BrotliDivansHybridCompressor<SelectedCDF,
                                    ChosenEncoder,
                                    AllocU8,
                                    AllocU16,
                                    AllocU32,
                                    AllocI32,
                                    AllocCommand,
                                    AllocCDF2,
                                    AllocCDF16,
                                    AllocF64,
                                    AllocFV,
                                    AllocHL,
                                    AllocHC,
                                    AllocHD,
                                    AllocHP,
                                    AllocCT,
                                    AllocHT> {
    pub fn get_m8(&mut self) -> &mut AllocU8 {
       self.codec.get_m8()
    }
    fn divans_encode_commands<SliceType:SliceWrapper<u8>+Default>(cmd:&[brotli::interface::Command<SliceType>],
                                                          header_progress: &mut usize,
                                                          data:&mut ResizableByteBuffer<u8, AllocU8>,
                                                          codec: &mut DivansCodec<ChosenEncoder,
                                                                                  EncoderSpecialization,
                                                                                  SelectedCDF,
                                                                                  AllocU8,
                                                                                  AllocCDF2,
                                                                                  AllocCDF16>,
                                                          window_size: u8) {
        let mut cmd_offset = 0usize;
        loop {
            let ret: BrotliResult;
            let mut output_offset = 0usize;
            {
                let output = data.checkout_next_buffer(codec.get_m8(),
                                                           Some(interface::HEADER_LENGTH + 256));
                if *header_progress != interface::HEADER_LENGTH {
                    match write_header(header_progress, window_size, output, &mut output_offset) {
                        BrotliResult::ResultSuccess => {},
                        _ => panic!("Unexpected failure writing header"),
                    }
                }
                let mut unused: usize = 0;
                ret = codec.encode_or_decode(&[],
                                             &mut unused,
                                             output,
                                             &mut output_offset,
                                             cmd,
                                             &mut cmd_offset);
            }
            match ret {
                BrotliResult::ResultSuccess | BrotliResult::NeedsMoreInput => {
                    assert_eq!(cmd_offset, cmd.len());
                    data.commit_next_buffer(output_offset);
                    return;
                },
                BrotliResult::ResultFailure => panic!("Unexpected error code"),
                BrotliResult::NeedsMoreOutput => {
                    data.commit_next_buffer(output_offset);
                }
            }
        }
    }
    fn internal_encode_stream(&mut self,
                              op: BrotliEncoderOperation,
                              input:&[u8], input_offset: &mut usize) -> brotli::BrotliResult {
        let mut nothing : Option<usize> = None;
        {
            let divans_data_ref = &mut self.divans_data;
            let divans_codec_ref = &mut self.codec;
            let header_progress_ref = &mut self.header_progress;
            let window_size = self.window_size;
            let mut closure = |a:&[brotli::interface::Command<brotli::InputReference>]| Self::divans_encode_commands(a,
                                                                                                                 header_progress_ref,
                                                                                                                 divans_data_ref,
                                                                                                                 divans_codec_ref,
                                                                                                                 window_size);
            {
                let mut available_in = input.len() - *input_offset;
                let mut brotli_out_offset = 0usize;
                {
                    let brotli_buffer = self.brotli_data.checkout_next_buffer(&mut self.brotli_encoder.m8, Some(256));
                    let mut available_out = brotli_buffer.len();

                    if BrotliEncoderCompressStream(&mut self.brotli_encoder,
                                                   &mut self.mf64,
                                                   &mut self.mfv,
                                                   &mut self.mhl,
                                                   &mut self.mhc,
                                                   &mut self.mhd,
                                                   &mut self.mhp,
                                                   &mut self.mct,
                                                   &mut self.mht,
                                                   op,
                                                   &mut available_in,
                                                   input,
                                                   input_offset,
                                                   &mut available_out,
                                                   brotli_buffer,
                                                   &mut brotli_out_offset,
                                                   &mut nothing,
                                                   &mut closure) <= 0 {
                        return BrotliResult::ResultFailure;
                    }
                }
                self.brotli_data.commit_next_buffer(brotli_out_offset);
                if available_in != 0 || BrotliEncoderIsFinished(&mut self.brotli_encoder) == 0 {
                    return BrotliResult::NeedsMoreInput;
                }
            }
        }
        loop { // flush divans coder
            let ret;
            let mut output_offset = 0usize;
            {
                let mut output = self.divans_data.checkout_next_buffer(self.codec.get_m8(),
                                                                       Some(interface::HEADER_LENGTH + 256));
                ret = self.codec.flush(&mut output, &mut output_offset);
            }
            self.divans_data.commit_next_buffer(output_offset);
            match ret {
                            BrotliResult::ResultSuccess => return ret,
                BrotliResult::NeedsMoreOutput => {},
                BrotliResult::NeedsMoreInput | BrotliResult::ResultFailure => return BrotliResult::ResultFailure,
            }
        }
    }
    pub fn free(mut self) -> (AllocU8, AllocU32, AllocCDF2, AllocCDF16, AllocU8, AllocU16, AllocI32, AllocCommand,
                              AllocF64, AllocFV, AllocHL, AllocHC, AllocHD, AllocHP, AllocCT, AllocHT) {
        self.brotli_data.free(&mut self.brotli_encoder.m8);
        self.divans_data.free(&mut self.codec.get_m8());
        let (m8, mcdf2, mcdf16) = self.codec.free();
        brotli::enc::encode::BrotliEncoderDestroyInstance(&mut self.brotli_encoder);
        (m8, self.brotli_encoder.m32, mcdf2, mcdf16, self.brotli_encoder.m8, self.brotli_encoder.m16,self.brotli_encoder.mi32, self.brotli_encoder.mc,
         self.mf64, self.mfv, self.mhl, self.mhc, self.mhd, self.mhp, self.mct, self.mht)
    }
}

impl<SelectedCDF:CDF16,
     ChosenEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocU16:Allocator<u16>,
     AllocU32:Allocator<u32>,
     AllocI32:Allocator<i32>,
     AllocCommand:Allocator<super::brotli::enc::command::Command>,
     AllocCDF2:Allocator<CDF2>,
     AllocCDF16:Allocator<SelectedCDF>,
     AllocF64: Allocator<brotli::enc::util::floatX>,
     AllocFV: Allocator<brotli::enc::vectorization::Mem256f>,
     AllocHL: Allocator<brotli::enc::histogram::HistogramLiteral>,
     AllocHC: Allocator<brotli::enc::histogram::HistogramCommand>,
     AllocHD: Allocator<brotli::enc::histogram::HistogramDistance>,
     AllocHP: Allocator<brotli::enc::cluster::HistogramPair>,
     AllocCT: Allocator<brotli::enc::histogram::ContextType>,
     AllocHT: Allocator<brotli::enc::entropy_encode::HuffmanTree>
     > Compressor for BrotliDivansHybridCompressor<SelectedCDF,
                                                   ChosenEncoder,
                                                   AllocU8,
                                                   AllocU16,
                                                   AllocU32,
                                                   AllocI32,
                                                   AllocCommand,
                                                   AllocCDF2,
                                                   AllocCDF16,
                                                   AllocF64,
                                                   AllocFV,
                                                   AllocHL,
                                                   AllocHC,
                                                   AllocHD,
                                                   AllocHP,
                                                   AllocCT,
                                                   AllocHT> {
    fn encode(&mut self,
              input: &[u8],
              input_offset: &mut usize,
              _output: &mut [u8],
              _output_offset: &mut usize) -> BrotliResult {
        match self.internal_encode_stream(BrotliEncoderOperation::BROTLI_OPERATION_PROCESS,
                                          input,
                                          input_offset) {
            BrotliResult::ResultFailure => BrotliResult::ResultFailure,
            BrotliResult::ResultSuccess | BrotliResult::NeedsMoreInput => BrotliResult::NeedsMoreInput,
            BrotliResult::NeedsMoreOutput => panic!("unexpected code"),
        }
    }
    fn flush(&mut self,
             output: &mut [u8],
             output_offset: &mut usize) -> BrotliResult {
        let mut zero = 0usize;
        match self.internal_encode_stream(BrotliEncoderOperation::BROTLI_OPERATION_FINISH,
                                          &[],
                                          &mut zero) {
            BrotliResult::ResultFailure => return BrotliResult::ResultFailure,
            BrotliResult::ResultSuccess => {}
            BrotliResult::NeedsMoreOutput | BrotliResult::NeedsMoreInput => panic!("unexpected code"),
        }
        // we're in success area here
        let destination = output.split_at_mut(*output_offset).1;
        let src = self.divans_data.slice().split_at(self.encoded_byte_offset).1;
        let copy_len = min(src.len(), destination.len());
        destination.split_at_mut(copy_len).0.clone_from_slice(src.split_at(copy_len).0);
        *output_offset += copy_len;
        self.encoded_byte_offset += copy_len;
        if self.encoded_byte_offset == self.divans_data.len() {
            return BrotliResult::ResultSuccess;
        }
        BrotliResult::NeedsMoreOutput
    }
    fn encode_commands<SliceType:SliceWrapper<u8>+Default>(&mut self,
                                                           input:&[Command<SliceType>],
                                                           input_offset : &mut usize,
                                                           output :&mut[u8],
                                                           output_offset: &mut usize) -> BrotliResult {
        if self.header_progress != interface::HEADER_LENGTH {
            match write_header(&mut self.header_progress, self.window_size, output, output_offset) {
                BrotliResult::ResultSuccess => {},
                res => return res,
            }
        }
        let mut unused: usize = 0;
        self.codec.encode_or_decode(&[],
                                    &mut unused,
                                    output,
                                    output_offset,
                                    input,
                                    input_offset)
    }
}

pub struct BrotliDivansHybridCompressorFactory<AllocU8:Allocator<u8>,
     AllocU16:Allocator<u16>,
     AllocU32:Allocator<u32>,
     AllocI32:Allocator<i32>,
     AllocCommand:Allocator<super::brotli::enc::command::Command>,
     AllocCDF2:Allocator<CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocF64: Allocator<brotli::enc::util::floatX>,
     AllocFV: Allocator<brotli::enc::vectorization::Mem256f>,
     AllocHL: Allocator<brotli::enc::histogram::HistogramLiteral>,
     AllocHC: Allocator<brotli::enc::histogram::HistogramCommand>,
     AllocHD: Allocator<brotli::enc::histogram::HistogramDistance>,
     AllocHP: Allocator<brotli::enc::cluster::HistogramPair>,
     AllocCT: Allocator<brotli::enc::histogram::ContextType>,
     AllocHT: Allocator<brotli::enc::entropy_encode::HuffmanTree>> {
    p1: PhantomData<AllocU8>,
    p2: PhantomData<AllocCDF2>,
    p3: PhantomData<AllocCDF16>,
    p4: PhantomData<AllocU16>,
    p5: PhantomData<AllocU32>,
    p6: PhantomData<AllocI32>,
    p7: PhantomData<AllocCommand>,
    p8: PhantomData<AllocF64>,
    p9: PhantomData<AllocFV>,
    pa: PhantomData<AllocHL>,
    pb: PhantomData<AllocHC>,
    pc: PhantomData<AllocHD>,
    pd: PhantomData<AllocHP>,
    pe: PhantomData<AllocCT>,
    pf: PhantomData<AllocHT>,
}
impl<AllocU8:Allocator<u8>,
     AllocU16:Allocator<u16>,
     AllocI32:Allocator<i32>,
     AllocCommand:Allocator<super::brotli::enc::command::Command>,
     AllocU32:Allocator<u32>,
     AllocCDF2:Allocator<CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocF64: Allocator<brotli::enc::util::floatX>,
     AllocFV: Allocator<brotli::enc::vectorization::Mem256f>,
     AllocHL: Allocator<brotli::enc::histogram::HistogramLiteral>,
     AllocHC: Allocator<brotli::enc::histogram::HistogramCommand>,
     AllocHD: Allocator<brotli::enc::histogram::HistogramDistance>,
     AllocHP: Allocator<brotli::enc::cluster::HistogramPair>,
     AllocCT: Allocator<brotli::enc::histogram::ContextType>,
     AllocHT: Allocator<brotli::enc::entropy_encode::HuffmanTree>> interface::DivansCompressorFactory<AllocU8, AllocU32, AllocCDF2, AllocCDF16>
    for BrotliDivansHybridCompressorFactory<AllocU8, AllocU16, AllocU32, AllocI32, AllocCommand, AllocCDF2, AllocCDF16,
                                            AllocF64, AllocFV, AllocHL, AllocHC, AllocHD, AllocHP, AllocCT, AllocHT> {
     type DefaultEncoder = DefaultEncoderType!();
     type ConstructedCompressor = BrotliDivansHybridCompressor<interface::DefaultCDF16,
                                                               Self::DefaultEncoder,
                                                               AllocU8,
                                                               AllocU16,
                                                               AllocU32,
                                                               AllocI32,
                                                               AllocCommand,
                                                               AllocCDF2,
                                                               AllocCDF16,
                                                               AllocF64,
                                                               AllocFV,
                                                               AllocHL,
                                                               AllocHC,
                                                               AllocHD,
                                                               AllocHP,
                                                               AllocCT,
                                                               AllocHT>;
      type AdditionalArgs = (AllocU8, AllocU16, AllocI32, AllocCommand,
                             AllocF64, AllocFV, AllocHL, AllocHC, AllocHD, AllocHP, AllocCT, AllocHT);
     fn new(mut m8: AllocU8, m32: AllocU32, mcdf2:AllocCDF2, mcdf16:AllocCDF16,mut window_size: usize,
           literal_adaptation_rate: Option<Speed>,
           additional_args: Self::AdditionalArgs) -> Self::ConstructedCompressor {
        if window_size < 10 {
            window_size = 10;
        }
        if window_size > 24 {
            window_size = 24;
        }
        let enc = Self::DefaultEncoder::new(&mut m8);
         let mut ret = Self::ConstructedCompressor {
             mf64: additional_args.4,
             mfv: additional_args.5,
             mhl: additional_args.6,
             mhc: additional_args.7,
             mhd: additional_args.8,
             mhp: additional_args.9,
             mct: additional_args.10,
             mht: additional_args.11,
             brotli_data: ResizableByteBuffer::<u8, AllocU8>::new(),
             divans_data: ResizableByteBuffer::<u8, AllocU8>::new(),
             encoded_byte_offset:0, 
             brotli_encoder: brotli::enc::encode::BrotliEncoderCreateInstance(additional_args.0,
                                                                              additional_args.1,
                                                                              additional_args.2,
                                                                              m32,
                                                                              additional_args.3),
            codec:DivansCodec::<Self::DefaultEncoder, EncoderSpecialization, interface::DefaultCDF16, AllocU8, AllocCDF2, AllocCDF16>::new(
                m8,
                mcdf2,
                mcdf16,
                enc,
                EncoderSpecialization::new(),
                window_size,
                literal_adaptation_rate,
            ),
            header_progress: 0,
            window_size: window_size as u8,
        };
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_LGWIN,
                                                       window_size as u32);
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_LGBLOCK,
                                                       1024 * 1024);
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_QUALITY,
                                                       10);
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_METABLOCK_CALLBACK,
                                                       1);
        ret
    }
}
