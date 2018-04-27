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

pub use interface::{DivansResult, DivansOutputResult};
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
use brotli::dictionary::{kBrotliMaxDictionaryWordLength, kBrotliDictionary,
                                      kBrotliDictionaryOffsetsByLength};
use brotli::TransformDictionaryWord;
pub use super::interface::{Command, Compressor, LiteralCommand, CopyCommand, DictCommand, FeatureFlagSliceType};
mod test;
pub struct DivansRecodeState<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8>>{
    total_offset: usize,
    input_sub_offset: usize,
    pub ring_buffer: RingBuffer,
    ring_buffer_decode_index: u32,
    ring_buffer_output_index: u32,
}

const REPEAT_BUFFER_MAX_SIZE: u32 = 64;

impl<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8> + Default> Default for DivansRecodeState<RingBuffer> {
   fn default() -> Self {
      DivansRecodeState::<RingBuffer>::new(RingBuffer::default())
   }
}
impl<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8>> DivansRecodeState<RingBuffer> {
    pub fn new(rb:RingBuffer) -> Self {
        DivansRecodeState {
            ring_buffer: rb,
            ring_buffer_decode_index: 0,
            ring_buffer_output_index: 0,
            input_sub_offset: 0,
            total_offset:0,
        }
    }
    #[inline(always)]
    pub fn num_bytes_encoded(&self) -> usize {
        self.total_offset
    }
    #[cold]
    fn fallback_last_8_literals(&self) -> [u8; 8] {
        let len = self.ring_buffer.slice().len();
        let mut ret = [0u8; 8];
        for i in 0..8 {
            ret[i] = self.ring_buffer.slice()[(self.ring_buffer_decode_index as usize + len - i - 1) & (len - 1)];
        }
        ret
    }
    #[inline(always)]
    pub fn last_8_literals(&self) -> [u8; 8] {
        if self.ring_buffer_decode_index < 8 {
            self.fallback_last_8_literals()
        } else {
            let mut ret = [0u8; 8];
            ret.clone_from_slice(self.ring_buffer.slice().split_at(self.ring_buffer_decode_index as usize - 8).1.split_at(8).0);
            ret
        }
    }
    // this copies as much data as possible from the RingBuffer
    // it starts at the ring_buffer_output_index...and advances up to the ring_buffer_decode_index
    pub fn flush(&mut self, output :&mut[u8], output_offset: &mut usize) -> DivansOutputResult {
        if self.ring_buffer_decode_index < self.ring_buffer_output_index { // we wrap around
            let bytes_until_wrap = self.ring_buffer.slice().len() - self.ring_buffer_output_index as usize;
            let amount_to_copy = core::cmp::min(bytes_until_wrap, output.len() - *output_offset);
            output[*output_offset..(*output_offset + amount_to_copy)].clone_from_slice(
                &self.ring_buffer.slice()[self.ring_buffer_output_index as usize..(self.ring_buffer_output_index as usize
                                                                         + amount_to_copy)]);
            self.ring_buffer_output_index += amount_to_copy as u32;
            *output_offset += amount_to_copy;
            if self.ring_buffer_output_index as usize == self.ring_buffer.slice().len() {
               self.ring_buffer_output_index = 0;
            }
        }
        if *output_offset != output.len() && self.ring_buffer_output_index < self.ring_buffer_decode_index {
            let amount_to_copy = core::cmp::min((self.ring_buffer_decode_index - self.ring_buffer_output_index) as usize ,
                                                output.len() - *output_offset);
            
            output[*output_offset..(*output_offset + amount_to_copy)].clone_from_slice(
                &self.ring_buffer.slice()[self.ring_buffer_output_index as usize..(self.ring_buffer_output_index as usize+
                                                                 amount_to_copy)]);
            self.ring_buffer_output_index += amount_to_copy as u32;
            *output_offset += amount_to_copy;
            if self.ring_buffer_output_index as usize == self.ring_buffer.slice().len() {
               self.ring_buffer_output_index = 0;
            }           
        }
        if self.ring_buffer_output_index != self.ring_buffer_decode_index {
            return DivansOutputResult::NeedsMoreOutput;
        }
        DivansOutputResult::Success
    }
    fn decode_space_left_in_ring_buffer(&self) -> u32 {
        // tried optimizing with predicates but no luck: the branch wins here (largely coherent; does less work in the common case)
        // also do not inline: the branch predictor is forgetful about the branch here if this gets inlined everywhere
        if self.ring_buffer_output_index <= self.ring_buffer_decode_index {
            return self.ring_buffer_output_index + self.ring_buffer.slice().len() as u32 - 1 - self.ring_buffer_decode_index;
        }
        self.ring_buffer_output_index - 1 - self.ring_buffer_decode_index
    }
    fn copy_decoded_from_ring_buffer(&self, mut output: &mut[u8], mut distance: u32, mut amount_to_copy: u32) {
        if distance > self.ring_buffer_decode_index {
            // we need to copy this in two segments...starting with the segment far past the end
            let far_distance = distance - self.ring_buffer_decode_index;
            let far_start_index = self.ring_buffer.slice().len() as u32 - far_distance;
            let local_ring = self.ring_buffer.slice().split_at(far_start_index as usize).1;
            let far_amount = core::cmp::min(far_distance,
                                            amount_to_copy);
            let (output_far, output_near) = core::mem::replace(&mut output, &mut[]).split_at_mut(far_amount as usize);
            output_far.clone_from_slice(local_ring.split_at(far_amount as usize).0);
            output = output_near;
            distance = self.ring_buffer_decode_index;
            amount_to_copy -= far_amount as u32;
        }
        if !output.is_empty() {
            let start = self.ring_buffer_decode_index - distance;
            output.split_at_mut(amount_to_copy
                                as usize).0.clone_from_slice(self.ring_buffer.slice().split_at(start as usize).1.split_at(amount_to_copy
                                                                                                                 as usize).0);
        }
    }

    //precondition: that there is sufficient room for amount_to_copy in buffer
    fn copy_some_decoded_from_ring_buffer_to_decoded(&mut self, distance: u32, mut desired_amount_to_copy: u32) -> Result<u32,()> {
        desired_amount_to_copy = core::cmp::min(self.decode_space_left_in_ring_buffer() as u32,
                                                desired_amount_to_copy);
        let left_dst_before_wrap = self.ring_buffer.slice().len() as u32 - self.ring_buffer_decode_index;
        let mut src_distance_index :u32;
        if self.ring_buffer_decode_index as u32 >= distance {
            src_distance_index = self.ring_buffer_decode_index - distance;
        } else {
            src_distance_index = self.ring_buffer_decode_index + self.ring_buffer.slice().len() as u32;
            if src_distance_index >= distance {
                src_distance_index -= distance;
            } else {
                return Err(())
            }
        }
        let left_src_before_wrap = self.ring_buffer.slice().len() as u32 - src_distance_index;
        let mut trunc_amount_to_copy = core::cmp::min(core::cmp::min(left_dst_before_wrap,
                                                                 left_src_before_wrap),
                                                  desired_amount_to_copy);
        if src_distance_index < self.ring_buffer_decode_index {
            let (_unused, src_and_dst) = self.ring_buffer.slice_mut().split_at_mut(src_distance_index as usize);
            let (src, dst) = src_and_dst.split_at_mut((self.ring_buffer_decode_index - src_distance_index) as usize);
            dst.split_at_mut(trunc_amount_to_copy as usize).0.clone_from_slice(src.split_at_mut(trunc_amount_to_copy as usize).0);
        } else {
            let (_unused, dst_and_src) = self.ring_buffer.slice_mut().split_at_mut(self.ring_buffer_decode_index as usize);
            let (dst, src) = dst_and_src.split_at_mut((src_distance_index - self.ring_buffer_decode_index) as usize);
            trunc_amount_to_copy = core::cmp::min(trunc_amount_to_copy, core::cmp::min(dst.len(),
                                                                                       src.len()) as u32);
            dst.split_at_mut(trunc_amount_to_copy as usize).0.clone_from_slice(src.split_at_mut(trunc_amount_to_copy as usize).0);            
        }
        self.ring_buffer_decode_index += trunc_amount_to_copy;
        if self.ring_buffer_decode_index == self.ring_buffer.slice().len() as u32 {
            self.ring_buffer_decode_index =0;
        }
        Ok(trunc_amount_to_copy)
    }

    // takes in a buffer of data to copy to the ring buffer--returns the number of bytes persisted
    fn copy_to_ring_buffer(&mut self, mut data: &[u8]) -> usize {
        data = data.split_at(core::cmp::min(data.len() as u32, self.decode_space_left_in_ring_buffer()) as usize).0;
        let mut retval = 0usize;
        let first_section = self.ring_buffer.slice_mut().len() as u32 - self.ring_buffer_decode_index;
        let amount_to_copy = core::cmp::min(data.len() as u32, first_section);
        let (data_first, data_second) = data.split_at(amount_to_copy as usize);
        self.ring_buffer.slice_mut()[self.ring_buffer_decode_index as usize .. (self.ring_buffer_decode_index + amount_to_copy) as usize].clone_from_slice(data_first);
        self.ring_buffer_decode_index += amount_to_copy as u32;
        retval += amount_to_copy as usize;
        if self.ring_buffer_decode_index == self.ring_buffer.slice().len() as u32 {
            self.ring_buffer_decode_index = 0;
            let second_amount_to_copy = data_second.len();
            self.ring_buffer.slice_mut()[self.ring_buffer_decode_index as usize .. (self.ring_buffer_decode_index as usize + second_amount_to_copy)].clone_from_slice(data_second.split_at(second_amount_to_copy).0);
            self.ring_buffer_decode_index += second_amount_to_copy as u32;
            retval += second_amount_to_copy;
        }
        retval
    }
    fn parse_literal(&mut self, data:&[u8]) -> DivansOutputResult {
       let data_len = data.len(); 
        if data_len < self.input_sub_offset { // this means user passed us different data a second time
           return DivansOutputResult::Failure;
       }
       let remainder = data.split_at(self.input_sub_offset).1;
       let bytes_copied = self.copy_to_ring_buffer(remainder);
       self.input_sub_offset += bytes_copied as usize;
       if bytes_copied != remainder.len() {
          return DivansOutputResult::NeedsMoreOutput;
       }
       DivansOutputResult::Success
    }
    #[allow(unused)]
    fn parse_copy_simplified(&mut self, copy:&CopyCommand) -> DivansOutputResult {
        for i in (self.input_sub_offset as usize)..(copy.num_bytes as usize){
            if ((self.ring_buffer_decode_index + 1) & (self.ring_buffer.slice().len() as u32 - 1)) == self.ring_buffer_output_index {
               self.input_sub_offset = i;
               return DivansOutputResult::NeedsMoreOutput;
            }
            let mut src = self.ring_buffer_decode_index + self.ring_buffer.slice().len() as u32 - copy.distance;
            src &= self.ring_buffer.slice().len() as u32 - 1;
            let src_val = self.ring_buffer.slice()[src as usize];
            self.ring_buffer.slice_mut()[self.ring_buffer_decode_index as usize] = src_val;
            self.ring_buffer_decode_index += 1;
            if self.ring_buffer_decode_index == self.ring_buffer.slice().len() as u32 {
               self.ring_buffer_decode_index = 0;
            }
        }
        self.input_sub_offset = copy.num_bytes as usize;
        DivansOutputResult::Success
    }
    fn parse_copy(&mut self, copy:&CopyCommand) -> DivansOutputResult {
        let num_bytes_left_in_cmd = copy.num_bytes - self.input_sub_offset as u32;
        if copy.distance <= REPEAT_BUFFER_MAX_SIZE && num_bytes_left_in_cmd > copy.distance {
            let num_bytes_to_copy = core::cmp::min(num_bytes_left_in_cmd,
                                                   self.decode_space_left_in_ring_buffer());
            let mut repeat_alloc_buffer = [0u8;REPEAT_BUFFER_MAX_SIZE as usize];
            let repeat_buffer = repeat_alloc_buffer.split_at_mut(copy.distance as usize).0;
            self.copy_decoded_from_ring_buffer(repeat_buffer, copy.distance, copy.distance);
            let num_repeat_iter = num_bytes_to_copy / copy.distance;
            let rem_bytes = num_bytes_to_copy - num_repeat_iter * copy.distance;
            for _i in 0..num_repeat_iter {
                let ret = self.copy_to_ring_buffer(repeat_buffer);
                self.input_sub_offset += ret;
                if ret != repeat_buffer.len() {
                    return DivansOutputResult::NeedsMoreOutput;
                }
            }
            let ret = self.copy_to_ring_buffer(repeat_buffer.split_at(rem_bytes as usize).0) as u32;
            self.input_sub_offset += ret as usize;
            if ret != rem_bytes || num_bytes_to_copy != num_bytes_left_in_cmd {
                return DivansOutputResult::NeedsMoreOutput;
            }
            return DivansOutputResult::Success;
        }
        let num_bytes_to_copy = core::cmp::min(num_bytes_left_in_cmd, copy.distance);
        let copy_count = match self.copy_some_decoded_from_ring_buffer_to_decoded(
            copy.distance,
            num_bytes_to_copy) {
            Ok(copy_count) => copy_count,
            Err(_) => return DivansOutputResult::Failure,
        };
        self.input_sub_offset += copy_count as usize;
        // by taking the min of copy.distance and items to copy, we are nonoverlapping
        // this means we can use split_at_mut to cut the array into nonoverlapping segments
        if copy_count != num_bytes_left_in_cmd {
            return DivansOutputResult::NeedsMoreOutput;
        }
        DivansOutputResult::Success
    }
    fn parse_dictionary(&mut self, dict_cmd:&DictCommand) -> DivansOutputResult {
        // dictionary words are bounded in size: make sure there's enough room for the whole word
        let copy_len = u32::from(dict_cmd.word_size);
        let word_len_category_index = kBrotliDictionaryOffsetsByLength[copy_len as usize] as u32;
        let word_index = (dict_cmd.word_id * copy_len) + word_len_category_index;
        let dict = &kBrotliDictionary;
        let word = &dict[(word_index as usize)..(word_index as usize + copy_len as usize)];
        let mut transformed_word = [0u8;kBrotliMaxDictionaryWordLength as usize + 13];
        let final_len = TransformDictionaryWord(&mut transformed_word[..],
                                                &word[..],
                                                copy_len as i32,
                                                i32::from(dict_cmd.transform));
        if self.decode_space_left_in_ring_buffer() < final_len as u32 {
            return DivansOutputResult::NeedsMoreOutput;
        }
        if dict_cmd.final_size != 0 && final_len as usize != dict_cmd.final_size as usize {
            return DivansOutputResult::Failure;
        }
        if self.input_sub_offset != 0 {
            assert_eq!(self.input_sub_offset as i32, final_len);
        } else if self.copy_to_ring_buffer(transformed_word.split_at(final_len as usize).0) as i32 != final_len {
            panic!("We already assured sufficient space in buffer for word: internal error");
        }
        self.input_sub_offset = final_len as usize;
        DivansOutputResult::Success
    }
    fn parse_command<SliceType:SliceWrapper<u8>>(&mut self, cmd: &Command<SliceType>) -> DivansOutputResult {
        match *cmd {
              Command::Copy(ref copy) => self.parse_copy(copy),
              Command::Dict(ref dict) => self.parse_dictionary(dict),
              Command::Literal(ref literal) => self.parse_literal(literal.slice()),
              Command::PredictionMode(_)
              | Command::BlockSwitchCommand(_)
              | Command::BlockSwitchDistance(_)
              | Command::BlockSwitchLiteral(_) => DivansOutputResult::Success,
        }
    }
    pub fn encode_cmd<SliceType:SliceWrapper<u8>>(&mut self,
                  cmd:&Command<SliceType>,
                  output :&mut[u8],
                  output_offset: &mut usize) -> DivansOutputResult {
        loop {
            let prev_output_offset = *output_offset;
            let res = self.parse_command(cmd);
            match res {
                DivansOutputResult::Success => {
                    break;
                }, // move on to the next command
                DivansOutputResult::NeedsMoreOutput => {
                    match self.flush(output, output_offset) {
                        DivansOutputResult::Success => {},
                        flush_res => {
                            self.total_offset += *output_offset - prev_output_offset;
                            return flush_res
                        },
                    }
                }, // flush, and try again
                DivansOutputResult::Failure => return res,
            }
        }
        let prev_output_offset = *output_offset;
        match self.flush(output, output_offset)  {
            DivansOutputResult::Success => {
                self.input_sub_offset = 0;
                self.total_offset += *output_offset - prev_output_offset;
                DivansOutputResult::Success
            },
            res => {
                self.total_offset += *output_offset - prev_output_offset;
                res
            },
        }
    }
}
impl<RingBuffer:SliceWrapperMut<u8> + SliceWrapper<u8> + Default> Compressor for DivansRecodeState<RingBuffer> {
    fn encode(&mut self, input:&[u8], input_offset: &mut usize, output: &mut [u8], output_offset: &mut usize) -> DivansResult {
       let amt_to_copy = core::cmp::min(input.len() - *input_offset, output.len() - *output_offset);
       output.split_at_mut(*output_offset).1.split_at_mut(amt_to_copy).0.clone_from_slice(input.split_at(*input_offset).1.split_at(amt_to_copy).0);
       *input_offset += amt_to_copy;
       *output_offset += amt_to_copy;
       if *input_offset == input.len() {
          return DivansResult::Success;
       }
       if *output_offset == output.len() {
          return DivansResult::NeedsMoreOutput;
       }
       DivansResult::Failure
    }
    fn encode_commands<SliceType:SliceWrapper<u8>>(&mut self,
                  input:&[Command<SliceType>],
                  input_offset : &mut usize,
                  output :&mut[u8],
                  output_offset: &mut usize) -> DivansOutputResult {
        if *input_offset > input.len() {
            return DivansOutputResult::Failure;
        }
        for cmd in input.split_at(*input_offset).1.iter() {
            loop {
                let mut res = self.flush(output, output_offset);
                 match res {
                    DivansOutputResult::Success => {},
                    _ => {return res}
                 }
                 res = self.parse_command(cmd);
                 match res {
                    DivansOutputResult::Success => {
                        self.input_sub_offset = 0; // done w/this command, no partial work
                        break;
                    }, // move on to the next command
                    DivansOutputResult::NeedsMoreOutput => continue, // flush, and try again
                    DivansOutputResult::Failure => return res,
                 }
            }
            *input_offset += 1;
        }
        self.flush(output, output_offset)
    }
    fn flush(&mut self,
             _output:&mut[u8],
             _output_offset:&mut usize)->DivansOutputResult{
        DivansOutputResult::Success
    }
}
