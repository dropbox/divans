use core;
mod hash_match;
use self::hash_match::HashMatch;
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};

pub use brotli_decompressor::{BrotliResult};
pub use super::interface::{Command, Compressor, LiteralCommand, CopyCommand, DictCommand};
pub struct RawToCmdState<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8>,
    AllocU32:Allocator<u32>>{
    total_offset: usize,
    input_sub_offset: usize,
    pub ring_buffer: RingBuffer,
    ring_buffer_decode_index: u32,
    ring_buffer_output_index: u32,
    hash_match: HashMatch<AllocU32>,
}

impl<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8>, AllocU32:Allocator<u32>> RawToCmdState<RingBuffer, AllocU32> {
    pub fn new(m32:&mut AllocU32, rb:RingBuffer) -> Self {
        RawToCmdState {
            ring_buffer: rb,
            ring_buffer_decode_index: 0,
            ring_buffer_output_index: 0,
            input_sub_offset: 0,
            total_offset:0,
            hash_match:HashMatch::<AllocU32>::new(m32),
        }
    }
    pub fn stream<AllocU8:Allocator<u8>>(m8: &mut AllocU8,
              input:&[u8],
              input_offset:&mut usize,
              output: &mut [Command<AllocU8::AllocatedMemory>],
              output_offset:&mut usize) -> BrotliResult {
        BrotliResult::ResultFailure
    }
    pub fn flush<AllocU8:Allocator<u8>>(m8: &mut AllocU8,
              output: &mut [Command<AllocU8::AllocatedMemory>],
              output_offset:&mut usize) -> BrotliResult {
        BrotliResult::ResultFailure
    }
    pub fn free(&mut self, m32: &mut AllocU32) {
        self.hash_match.free(m32);
    }
}

