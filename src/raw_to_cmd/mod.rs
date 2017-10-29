// Copyright 2017 Dropbox, Inc
//
//   Licensed under the Apache License, Version 2.0 (the "License");
//   you may not use this file except in compliance with the License.
//   You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
//   Unless required by applicable law or agreed to in writing, software
//   distributed under the License is distributed on an "AS IS" BASIS,
//   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//   See the License for the specific language governing permissions and
//   limitations under the License.

use core;
mod hash_match;
use self::hash_match::HashMatch;
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use super::slice_util::SliceReference;
pub use brotli::BrotliResult;
pub use super::interface::{Command, Compressor, LiteralCommand, CopyCommand, DictCommand, FeatureFlagSliceType};
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
    /*
    fn freeze_dry<SliceType:SliceWrapper<u8>+Default>(&mut self, input:&[Command<SliceType>]) {
        
    }
    fn thaw<SliceType:SliceWrapper<u8>+Default>(&mut self, input:&[Command<SliceType>]) {
        
    }*/
    pub fn ring_buffer_full(&self) -> bool {
        self.ring_buffer_decode_index as usize == self.ring_buffer.slice().len() || self.ring_buffer_decode_index + 1 == self.ring_buffer_output_index
    }
    pub fn stream<'a>(&'a mut self,
                                          input:&[u8],
                                          input_offset:&mut usize,
                                          output: &mut [Command<SliceReference<'a, u8>>],
                                          output_offset:&mut usize) -> BrotliResult {
        if self.ring_buffer_decode_index >= self.ring_buffer_output_index {
            let max_copy = core::cmp::min(self.ring_buffer.slice().len() - self.ring_buffer_decode_index as usize,
                                          input.len() - *input_offset);
            self.ring_buffer.slice_mut()[(self.ring_buffer_decode_index as usize)..(self.ring_buffer_decode_index as usize + max_copy)].clone_from_slice(&input[*input_offset..(*input_offset + max_copy)]);
            *input_offset += max_copy;
            self.ring_buffer_decode_index += max_copy as u32;
            if self.ring_buffer_output_index != 0 {
               self.ring_buffer_decode_index = 0;
            }
        }
        if self.ring_buffer_decode_index < self.ring_buffer_output_index {
           let max_copy = core::cmp::min(self.ring_buffer_output_index as usize - 1,
                                         input.len() - *input_offset);
           self.ring_buffer.slice_mut()[(self.ring_buffer_decode_index as usize)..(self.ring_buffer_decode_index as usize + max_copy)].clone_from_slice(&input[*input_offset..(*input_offset + max_copy)]);
            *input_offset += max_copy;
            self.ring_buffer_decode_index += max_copy as u32;
        }
        if *output_offset < output.len() && self.ring_buffer_full() {
            match self.flush(output, output_offset) {
                BrotliResult::NeedsMoreOutput => {
                  return BrotliResult::NeedsMoreOutput;
                }
                BrotliResult::ResultFailure => {
                    return BrotliResult::ResultFailure;
                },
                _ => {
                    if *input_offset != input.len() {
                        // not really true: we may be able to consume more input, but ourr
                        // ring buffer is borrowed
                        return BrotliResult::NeedsMoreOutput;
                    }
                },
            }
        } else if *output_offset == output.len() {
            return BrotliResult::NeedsMoreOutput;
        }
        assert_eq!(*input_offset, input.len());
        BrotliResult::NeedsMoreInput
    }
    pub fn flush<'a>(
              &'a mut self,
              output: &mut [Command<SliceReference<'a, u8>>],
              output_offset:&mut usize) -> BrotliResult {
        if *output_offset == output.len() {
           return BrotliResult::NeedsMoreOutput;
        }
        if self.ring_buffer_decode_index < self.ring_buffer_output_index {
           let max_copy = self.ring_buffer.slice().len() - self.ring_buffer_output_index as usize;
           output[*output_offset] = Command::Literal(
               LiteralCommand::<SliceReference<'a, u8> >{
                   data: SliceReference::<u8>::new(self.ring_buffer.slice(),
                                                   self.ring_buffer_output_index as usize,
                                                   max_copy),
                   prob: FeatureFlagSliceType::<SliceReference<u8>>::default(),
                   
               });
           *output_offset += 1;
           if self.ring_buffer_decode_index as usize == self.ring_buffer.slice().len() {
               self.ring_buffer_decode_index = 0;
           }
           self.ring_buffer_output_index = 0
        }
        if self.ring_buffer_decode_index != self.ring_buffer_output_index {
           if *output_offset == output.len() {
               return BrotliResult::NeedsMoreOutput;
           }
           let max_copy = self.ring_buffer_decode_index as usize - self.ring_buffer_output_index as usize;
           output[*output_offset] = Command::Literal(
               LiteralCommand::<SliceReference<'a, u8>>{
                   data: SliceReference::<u8>::new(self.ring_buffer.slice(),
                                                   self.ring_buffer_output_index as usize,
                                                   max_copy),
                   prob: FeatureFlagSliceType::<SliceReference<u8>>::default(),
               });
           *output_offset += 1;
           self.ring_buffer_output_index = self.ring_buffer_decode_index
        }
        BrotliResult::ResultSuccess
    }
    pub fn stream2<AllocU8:Allocator<u8>>(&mut self,
              m8: &mut AllocU8,
              input:&[u8],
              input_offset:&mut usize,
              output: &mut [Command<AllocU8::AllocatedMemory>],
              output_offset:&mut usize) -> BrotliResult {
      while true {
        if self.ring_buffer_decode_index >= self.ring_buffer_output_index {
            let max_copy = core::cmp::min(self.ring_buffer.slice().len() - self.ring_buffer_decode_index as usize,
                                          input.len() - *input_offset);
            self.ring_buffer.slice_mut()[(self.ring_buffer_decode_index as usize)..(self.ring_buffer_decode_index as usize + max_copy)].clone_from_slice(&input[*input_offset..(*input_offset + max_copy)]);
            *input_offset += max_copy;
            self.ring_buffer_decode_index += max_copy as u32;
            if self.ring_buffer_output_index != 0 {
               self.ring_buffer_decode_index = 0;
            }
        }
        if self.ring_buffer_decode_index < self.ring_buffer_output_index {
           let max_copy = core::cmp::min(self.ring_buffer_output_index as usize - 1,
                                         input.len() - *input_offset);
           self.ring_buffer.slice_mut()[(self.ring_buffer_decode_index as usize)..(self.ring_buffer_decode_index as usize + max_copy)].clone_from_slice(&input[*input_offset..(*input_offset + max_copy)]);
            *input_offset += max_copy;
            self.ring_buffer_decode_index += max_copy as u32;
        }
        if *output_offset < output.len() && self.ring_buffer_full() {
           match self.flush2(m8, output, output_offset) {
              BrotliResult::NeedsMoreOutput => {
                  return BrotliResult::NeedsMoreOutput;
              }
              BrotliResult::ResultFailure => {
                  return BrotliResult::ResultFailure;
              },
             _ => {},
           }
        } else if *output_offset == output.len() {
           return BrotliResult::NeedsMoreOutput;
        } else {
           assert_eq!(*input_offset, input.len());
           break;
        }
      }
      BrotliResult::NeedsMoreInput
    }
    pub fn flush2<AllocU8:Allocator<u8>>(
              &mut self,
              m8: &mut AllocU8,
              output: &mut [Command<AllocU8::AllocatedMemory>],
              output_offset:&mut usize) -> BrotliResult {
        if *output_offset == output.len() {
           return BrotliResult::NeedsMoreOutput;
        }
        if self.ring_buffer_decode_index < self.ring_buffer_output_index {
           let max_copy = self.ring_buffer.slice().len() - self.ring_buffer_output_index as usize;
           let mut data_slice = m8.alloc_cell(max_copy);
           data_slice.slice_mut()[..max_copy].clone_from_slice(self.ring_buffer.slice().split_at(self.ring_buffer_output_index as usize).1);
           output[*output_offset] = Command::Literal(
               LiteralCommand::<AllocU8::AllocatedMemory>{
                  data: data_slice,
                  prob: FeatureFlagSliceType::<AllocU8::AllocatedMemory>::default(),
               });
           *output_offset += 1;
           if self.ring_buffer_decode_index as usize == self.ring_buffer.slice().len() {
               self.ring_buffer_decode_index = 0;
           }
           self.ring_buffer_output_index = 0
        }
        if self.ring_buffer_decode_index != self.ring_buffer_output_index {
           if *output_offset == output.len() {
               return BrotliResult::NeedsMoreOutput;
           }
           let max_copy = self.ring_buffer_decode_index as usize - self.ring_buffer_output_index as usize;
           let mut data_slice = m8.alloc_cell(max_copy);
           data_slice.slice_mut()[..max_copy].clone_from_slice(&self.ring_buffer.slice()[(self.ring_buffer_output_index as usize)..(self.ring_buffer_decode_index as usize)]);
           output[*output_offset] = Command::Literal(
               LiteralCommand::<AllocU8::AllocatedMemory>{
                   data: data_slice,
                   prob: FeatureFlagSliceType::<AllocU8::AllocatedMemory>::default(),
               });
           *output_offset += 1;
           self.ring_buffer_output_index = self.ring_buffer_decode_index
        }
        BrotliResult::ResultSuccess
    }
    pub fn free(&mut self, m32: &mut AllocU32) {
        self.hash_match.free(m32);
    }
}
