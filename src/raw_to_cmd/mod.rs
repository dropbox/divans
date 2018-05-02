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
pub use interface::{DivansResult, DivansOutputResult};
pub use super::interface::{PredictionModeContextMap, Command, Compressor, LiteralCommand, CopyCommand, DictCommand, FeatureFlagSliceType};
pub struct RawToCmdState<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8>,
    AllocU32:Allocator<u32>>{
    pub ring_buffer: RingBuffer,
    ring_buffer_decode_index: u32,
    ring_buffer_output_index: u32,
    hash_match: HashMatch<AllocU32>,
    pub has_produced_header: bool,
}

impl<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8>, AllocU32:Allocator<u32>> RawToCmdState<RingBuffer, AllocU32> {
    pub fn new(m32:&mut AllocU32, rb:RingBuffer) -> Self {
        RawToCmdState {
            ring_buffer: rb,
            ring_buffer_decode_index: 0,
            ring_buffer_output_index: 0,
            hash_match:HashMatch::<AllocU32>::new(m32),
            has_produced_header: false, // only produce header if no ir_translation
        }
    }
    pub fn raw_input_ir_mode(&mut self) {
        self.has_produced_header = true; // do not wish an additional prediction mode command at the end
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
                      output_offset:&mut usize,
                      literal_context_map: &'a mut[u8],
                      prediction_mode_backing:&'a mut[u8],
    ) -> DivansResult {
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
           let max_copy = core::cmp::min(self.ring_buffer_output_index as usize - 1 - self.ring_buffer_decode_index as usize,
                                         input.len() - *input_offset);
           debug_assert!(self.ring_buffer_output_index <= self.ring_buffer.slice().len() as u32);
           debug_assert!(self.ring_buffer_decode_index + max_copy  as u32 <= self.ring_buffer.slice().len() as u32);
           debug_assert!(*input_offset as u32 + max_copy as u32 <= input.len() as u32);
           self.ring_buffer.slice_mut()[(self.ring_buffer_decode_index as usize)..(self.ring_buffer_decode_index as usize + max_copy)].clone_from_slice(&input[*input_offset..(*input_offset + max_copy)]);
            *input_offset += max_copy;
            self.ring_buffer_decode_index += max_copy as u32;
        }
        if *output_offset < output.len() && self.ring_buffer_full() {
            match self.flush(output, output_offset, literal_context_map, prediction_mode_backing) {
                DivansOutputResult::NeedsMoreOutput => {
                  return DivansResult::NeedsMoreOutput;
                }
                DivansOutputResult::Failure(m) => {
                    return DivansResult::Failure(m);
                },
                _ => {
                    if *input_offset != input.len() {
                        // not really true: we may be able to consume more input, but ourr
                        // ring buffer is borrowed
                        return DivansResult::NeedsMoreOutput;
                    }
                },
            }
        } else if *output_offset == output.len() {
            return DivansResult::NeedsMoreOutput;
        }
        assert_eq!(*input_offset, input.len());
        DivansResult::NeedsMoreInput
    }
    pub fn flush<'a>(
              &'a mut self,
              output: &mut [Command<SliceReference<'a, u8>>],
              output_offset:&mut usize,
              literal_context_map: &'a mut[u8],
              prediction_mode_backing:&'a mut[u8]) -> DivansOutputResult {

        if *output_offset == output.len() {
           return DivansOutputResult::NeedsMoreOutput;
        }
        if !self.has_produced_header {
            self.has_produced_header = true;
            for (index, item) in literal_context_map.iter_mut().enumerate() {
                *item = index as u8 & 0x3f;
            }
            for (index, item) in prediction_mode_backing[super::interface::DISTANCE_CONTEXT_MAP_OFFSET..].iter_mut().enumerate() {
                *item = index as u8 & 0x3;
            }
            for item in prediction_mode_backing[super::interface::MIXING_OFFSET..super::interface::MIXING_OFFSET + super::interface::NUM_MIXING_VALUES].iter_mut() {
                *item = 4;
            }
            output[*output_offset] = Command::PredictionMode(
                PredictionModeContextMap::<SliceReference<'a, u8> >{
                    literal_context_map: SliceReference::<u8>::new(literal_context_map, 0, 64),
                        predmode_speed_and_distance_context_map: SliceReference::<u8>::new(prediction_mode_backing, 0, super::interface::DISTANCE_CONTEXT_MAP_OFFSET + 4),
                    });
            *output_offset += 1;
            if *output_offset == output.len() {
                return DivansOutputResult::NeedsMoreOutput;
            }
        }
        if self.ring_buffer_decode_index < self.ring_buffer_output_index {
           let max_copy = self.ring_buffer.slice().len() - self.ring_buffer_output_index as usize;
           if max_copy != 0 {
               output[*output_offset] = Command::Literal(
                   LiteralCommand::<SliceReference<'a, u8> >{
                       data: SliceReference::<u8>::new(self.ring_buffer.slice(),
                                                       self.ring_buffer_output_index as usize,
                                                       max_copy),
                       prob: FeatureFlagSliceType::<SliceReference<u8>>::default(),
                       high_entropy: false,
                   });
               *output_offset += 1;
           }
           if self.ring_buffer_decode_index as usize == self.ring_buffer.slice().len() {
               self.ring_buffer_decode_index = 0;
           }
           self.ring_buffer_output_index = 0
        }
        if self.ring_buffer_decode_index != self.ring_buffer_output_index {
           if *output_offset == output.len() {
               return DivansOutputResult::NeedsMoreOutput;
           }
           let max_copy = self.ring_buffer_decode_index as usize - self.ring_buffer_output_index as usize;
           output[*output_offset] = Command::Literal(
               LiteralCommand::<SliceReference<'a, u8>>{
                   data: SliceReference::<u8>::new(self.ring_buffer.slice(),
                                                   self.ring_buffer_output_index as usize,
                                                   max_copy),
                   prob: FeatureFlagSliceType::<SliceReference<u8>>::default(),
                   high_entropy: false,
               });
           *output_offset += 1;
           assert!(self.ring_buffer_output_index <= self.ring_buffer.slice().len() as u32);
           self.ring_buffer_output_index = self.ring_buffer_decode_index;
           assert!(self.ring_buffer_output_index <= self.ring_buffer.slice().len() as u32);
        }
        DivansOutputResult::Success
    }
    pub fn free(&mut self, m32: &mut AllocU32) {
        self.hash_match.free(m32);
    }
}
