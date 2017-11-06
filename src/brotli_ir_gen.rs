

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
pub struct BrotliDivansHybridCompressor<SelectedCDF:CDF16,
                            DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                            AllocU8:Allocator<u8>,
                            AllocU16:Allocator<u16>,
                            AllocU32:Allocator<u32>,
                            AllocI32:Allocator<i32>,
                            AllocCommand:Allocator<super::brotli::enc::command::Command>,
                            AllocCDF2:Allocator<CDF2>,
                            AllocCDF16:Allocator<SelectedCDF>> {
    m32: AllocU32,
    brotli_encoder: BrotliEncoderStateStruct<AllocU8, AllocU16, AllocU32, AllocI32, AllocCommand>,
    codec: DivansCodec<DefaultEncoder, EncoderSpecialization, SelectedCDF, AllocU8, AllocCDF2, AllocCDF16>,
    header_progress: usize,
    window_size: u8,
}
impl<SelectedCDF:CDF16,
     DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocU16:Allocator<u16>,
     AllocU32:Allocator<u32>,
     AllocI32:Allocator<i32>,
     AllocCommand:Allocator<super::brotli::enc::command::Command>,
     AllocCDF2:Allocator<CDF2>,
     AllocCDF16:Allocator<SelectedCDF>
     > Compressor for BrotliDivansHybridCompressor<SelectedCDF,
                                                   DefaultEncoder,
                                                   AllocU8,
                                                   AllocU16,
                                                   AllocU32,
                                                   AllocI32,
                                                   AllocCommand,
                                                   AllocCDF2,
                                                   AllocCDF16> {
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
        BrotliResult::ResultFailure        
    }
}

struct BrotliDivansHybridCompressorFactory<AllocU8:Allocator<u8>,
     AllocU16:Allocator<u16>,
     AllocU32:Allocator<u32>,
     AllocI32:Allocator<i32>,
     AllocCommand:Allocator<super::brotli::enc::command::Command>,
     AllocCDF2:Allocator<CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> {
    p1: PhantomData<AllocU8>,
    p2: PhantomData<AllocCDF2>,
    p3: PhantomData<AllocCDF16>,
    p4: PhantomData<AllocU16>,
    p5: PhantomData<AllocU32>,
    p6: PhantomData<AllocI32>,
    p7: PhantomData<AllocCommand>,
}
impl<AllocU8:Allocator<u8>,
     AllocU16:Allocator<u16>,
     AllocI32:Allocator<i32>,
     AllocCommand:Allocator<super::brotli::enc::command::Command>,
     AllocU32:Allocator<u32>,
     AllocCDF2:Allocator<CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> interface::DivansCompressorFactory<AllocU8, AllocU32, AllocCDF2, AllocCDF16>
    for BrotliDivansHybridCompressorFactory<AllocU8, AllocU16, AllocU32, AllocI32, AllocCommand, AllocCDF2, AllocCDF16> {
     type DefaultEncoder = DefaultEncoderType!();
     type ConstructedCompressor = BrotliDivansHybridCompressor<interface::DefaultCDF16,
                                                               Self::DefaultEncoder,
                                                               AllocU8,
                                                               AllocU16,
                                                               AllocU32,
                                                               AllocI32,
                                                               AllocCommand,
                                                               AllocCDF2,
                                                               AllocCDF16>;
     type AdditionalArgs = (AllocU8, AllocU16, AllocI32, AllocU32, AllocCommand,);
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
        let assembler = super::raw_to_cmd::RawToCmdState::new(&mut m32, ring_buffer);
        Self::ConstructedCompressor {
            m32 :m32,
            brotli_encoder: brotli::enc::encode::BrotliEncoderCreateInstance(additional_args.0,
                                                                             additional_args.1,
                                                                             additional_args.2,
                                                                             additional_args.3,
                                                                             additional_args.4),
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
        }
    }
}
