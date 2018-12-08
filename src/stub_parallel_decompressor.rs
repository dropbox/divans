#![cfg(not(feature="std"))]
pub use interface::{DivansCompressorFactory, BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType,
                    DefaultCDF16, DivansResult};
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use super::divans_decompressor::StaticCommand;
pub use core::marker::PhantomData;

pub struct ParallelDivansProcess<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                                 AllocU8:Allocator<u8>,
                                 AllocCDF16:Allocator<DefaultCDF16>,
                                 AllocCommand:Allocator<StaticCommand>> {
    p0: PhantomData<DefaultDecoder>,
    p1: PhantomData<AllocU8>,
    p2: PhantomData<AllocCDF16>,
    p3: PhantomData<AllocCommand>,
}

impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<DefaultCDF16>,
     AllocCommand:Allocator<StaticCommand>>
    ParallelDivansProcess<DefaultDecoder, AllocU8, AllocCDF16, AllocCommand> {

    pub fn new<T>(_header: &mut T, mut _window_size: usize) -> Self {
        unimplemented!();
    }
    pub fn decode(&mut self,
                  _input:&[u8],
                  _input_offset:&mut usize,
                  _output:&mut [u8],
                  _output_offset: &mut usize) -> DivansResult {
        unimplemented!();
    }
    pub fn free_ref(&mut self){
        unimplemented!();
    }
    pub fn free(self) -> (AllocU8, AllocCDF16, AllocCommand) {
        unimplemented!();
    }
}
