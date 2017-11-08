

use core::marker::PhantomData;
use super::probability::{CDF2,CDF16, Speed};
use super::brotli;
pub use super::alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use super::interface::{BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType};

pub use super::cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec};
use super::interface;
use super::brotli::BrotliResult;
use super::brotli::enc::encode::BrotliEncoderStateStruct;
use super::divans_compressor::write_header;
pub struct BrotliDivansHybridCompressor<SelectedCDF:CDF16,
                            DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
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
    codec: DivansCodec<DefaultEncoder, EncoderSpecialization, SelectedCDF, AllocU8, AllocCDF2, AllocCDF16>,
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
}
impl<SelectedCDF:CDF16,
     DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
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
                                    DefaultEncoder,
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
    fn internal_encode_stream(&mut self,
                              op: brotli::enc::encode::BrotliEncoderOperation,
                              input:&[u8], mut input_offset: &mut usize,
                              output :&mut [u8], mut output_offset: &mut usize) -> brotli::BrotliResult {
        let mut available_in = input.len() - *input_offset;
        let mut available_out = output.len() - *output_offset;
        let mut nothing : Option<usize> = None;
        let mut closure = |a:&[brotli::interface::Command<brotli::InputReference>]| ();
        if brotli::enc::encode::BrotliEncoderCompressStream(&mut self.brotli_encoder,
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
                                                         output,
                                                         &mut output_offset,
                                                         &mut nothing,
                                                            &mut closure) == 0 {

            if available_out != 0 {
                return BrotliResult::NeedsMoreInput;
            }
            if available_out == 0 {
                return BrotliResult::NeedsMoreOutput;
            }
        }
        BrotliResult::ResultSuccess
    }
    pub fn free(mut self) -> (AllocU8, AllocU32, AllocCDF2, AllocCDF16, AllocU8, AllocU16, AllocI32, AllocCommand,
                              AllocF64, AllocFV, AllocHL, AllocHC, AllocHD, AllocHP, AllocCT, AllocHT) {
        let (m8, mcdf2, mcdf16) = self.codec.free();
        brotli::enc::encode::BrotliEncoderDestroyInstance(&mut self.brotli_encoder);
        (m8, self.brotli_encoder.m32, mcdf2, mcdf16, self.brotli_encoder.m8, self.brotli_encoder.m16,self.brotli_encoder.mi32, self.brotli_encoder.mc,
         self.mf64, self.mfv, self.mhl, self.mhc, self.mhd, self.mhp, self.mct, self.mht)
    }
}

impl<SelectedCDF:CDF16,
     DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
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
                                                   DefaultEncoder,
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
              output: &mut [u8],
              output_offset: &mut usize) -> BrotliResult {
        BrotliResult::ResultFailure
    }
    fn flush(&mut self,
             output: &mut [u8],
             output_offset: &mut usize) -> BrotliResult {
        BrotliResult::ResultFailure
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

struct BrotliDivansHybridCompressorFactory<AllocU8:Allocator<u8>,
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
    pA: PhantomData<AllocHL>,
    pB: PhantomData<AllocHC>,
    pC: PhantomData<AllocHD>,
    pD: PhantomData<AllocHP>,
    pE: PhantomData<AllocCT>,
    pF: PhantomData<AllocHT>,
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
     fn new(mut m8: AllocU8, mut m32: AllocU32, mcdf2:AllocCDF2, mcdf16:AllocCDF16,mut window_size: usize,
           literal_adaptation_rate: Option<Speed>,
           additional_args: Self::AdditionalArgs) -> Self::ConstructedCompressor {
        if window_size < 10 {
            window_size = 10;
        }
        if window_size > 24 {
            window_size = 24;
        }
        let ring_buffer = m8.alloc_cell(1<<window_size);
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
        ret
    }
}
