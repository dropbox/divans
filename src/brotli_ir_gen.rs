use super::probability::{CDF2,CDF16};
use super::brotli;
pub use super::alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use super::interface::{BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType};

pub use super::cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec};
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
