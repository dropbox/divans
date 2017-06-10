extern crate core;
extern crate alloc_no_stdlib as alloc;
extern crate brotli_decompressor;
mod test;
pub use brotli_decompressor::{BrotliResult};
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
use brotli_decompressor::dictionary::{kBrotliMaxDictionaryWordLength, kBrotliDictionary,
                                      kBrotliDictionaryOffsetsByLength};
use brotli_decompressor::transform::{TransformDictionaryWord};

#[derive(Debug)]
pub struct CopyCommand {
    pub distance: u32,
    pub num_bytes: u32,
}

#[derive(Debug)]
pub struct DictCommand {
    pub word_size: u8,
    pub transform: u8,
    pub final_size: u8,
    pub _empty: u8,
    pub word_id: u32,
}

#[derive(Debug)]
pub struct LiteralCommand<SliceType:alloc::SliceWrapper<u8>> {
    pub data: SliceType,
}

#[derive(Debug)]
pub enum Command<SliceType:alloc::SliceWrapper<u8> > {
    Copy(CopyCommand),
    Dict(DictCommand),
    Literal(LiteralCommand<SliceType>),
}

impl<SliceType:alloc::SliceWrapper<u8> > Command<SliceType> {
    pub fn nop() -> Command<SliceType> {
        Command::Copy(CopyCommand{distance:1, num_bytes:0})
    }
}

pub struct DivansRecodeState<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8> + Default>{
    input_sub_offset: usize,
    ring_buffer: RingBuffer,
    ring_buffer_decode_index: u32,
    ring_buffer_output_index: u32,
}

const REPEAT_BUFFER_MAX_SIZE: u32 = 64;

impl<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8> + Default> Default for DivansRecodeState<RingBuffer> {
   fn default() -> Self {
      DivansRecodeState::<RingBuffer>::new()
   }
}
impl<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8> + Default> DivansRecodeState<RingBuffer> {
    fn new() -> Self {
        DivansRecodeState {
            ring_buffer: RingBuffer::default(),
            ring_buffer_decode_index: 0,
            ring_buffer_output_index: 0,
            input_sub_offset: 0,
        }
    }
    // this copies as much data as possible from the RingBuffer
    // it starts at the ring_buffer_output_index...and advances up to the ring_buffer_decode_index
    pub fn flush(&mut self, output :&mut[u8], output_offset: &mut usize) -> BrotliResult {
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
            return BrotliResult::NeedsMoreOutput;
        }
        BrotliResult::ResultSuccess
    }
    fn decode_space_left_in_ring_buffer(&self) -> u32 {
        if self.ring_buffer_output_index <= self.ring_buffer_decode_index {
            return self.ring_buffer_output_index + self.ring_buffer.slice().len() as u32 - 1 - self.ring_buffer_decode_index;
        }
        return self.ring_buffer_output_index - 1 - self.ring_buffer_decode_index;
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
        if output.len() != 0 {
            let start = self.ring_buffer_decode_index - distance;
            output.split_at_mut(amount_to_copy
                                as usize).0.clone_from_slice(self.ring_buffer.slice().split_at(start as usize).1.split_at(amount_to_copy
                                                                                                                 as usize).0);
        }
    }

    //precondition: that there is sufficient room for amount_to_copy in buffer
    fn copy_some_decoded_from_ring_buffer_to_decoded(&mut self, distance: u32, mut desired_amount_to_copy: u32) -> u32 {
        desired_amount_to_copy = core::cmp::min(self.decode_space_left_in_ring_buffer() as u32,
                                                desired_amount_to_copy);
        let left_dst_before_wrap = self.ring_buffer.slice().len() as u32 - self.ring_buffer_decode_index;
        let src_distance_index :u32;
        if self.ring_buffer_decode_index as u32 >= distance {
            src_distance_index = self.ring_buffer_decode_index - distance;
        } else {
            src_distance_index = self.ring_buffer_decode_index + self.ring_buffer.slice().len() as u32 - distance;
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
        return trunc_amount_to_copy;
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
    fn parse_literal<SliceType:alloc::SliceWrapper<u8>>(&mut self,
                                                        lit:&LiteralCommand<SliceType>) -> BrotliResult {
       let data = lit.data.slice();
       if data.len() < self.input_sub_offset { // this means user passed us different data a second time
           return BrotliResult::ResultFailure;
       }
       let remainder = data.split_at(self.input_sub_offset).1;
       let bytes_copied = self.copy_to_ring_buffer(remainder);
       if bytes_copied != remainder.len() {
          self.input_sub_offset += bytes_copied as usize;
          return BrotliResult::NeedsMoreOutput;
       }
       self.input_sub_offset = 0;
       BrotliResult::ResultSuccess
    }
    fn parse_copy(&mut self, copy:&CopyCommand) -> BrotliResult {
        for i in (self.input_sub_offset as usize)..(copy.num_bytes as usize){
            if (self.ring_buffer_decode_index + 1 & (self.ring_buffer.slice().len() as u32 - 1)) == self.ring_buffer_output_index {
               return BrotliResult::NeedsMoreOutput;
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
        return BrotliResult::ResultSuccess;
        let num_bytes_left_in_cmd = copy.num_bytes - self.input_sub_offset as u32;
        if copy.distance <= REPEAT_BUFFER_MAX_SIZE && num_bytes_left_in_cmd > copy.distance {
            let num_bytes_to_copy = core::cmp::min(num_bytes_left_in_cmd,
                                                   self.decode_space_left_in_ring_buffer());
            let mut repeat_alloc_buffer = [0u8;REPEAT_BUFFER_MAX_SIZE as usize];
            let mut repeat_buffer = repeat_alloc_buffer.split_at_mut(copy.distance as usize).0;
            self.copy_decoded_from_ring_buffer(repeat_buffer, copy.distance, copy.distance);
            let num_repeat_iter = num_bytes_to_copy / copy.distance;
            let rem_bytes = num_bytes_to_copy - num_repeat_iter * copy.distance;
            for _i in 0..num_repeat_iter {
                let ret = self.copy_to_ring_buffer(repeat_buffer);
                self.input_sub_offset += ret;
                if ret != repeat_buffer.len() {
                    return BrotliResult::NeedsMoreOutput;
                }
            }
            let ret = self.copy_to_ring_buffer(repeat_buffer.split_at(rem_bytes as usize).0) as u32;
            self.input_sub_offset += ret as usize;
            if ret != rem_bytes || num_bytes_to_copy != num_bytes_left_in_cmd {
                return BrotliResult::NeedsMoreOutput;
            }
            self.input_sub_offset = 0; // we're done
            return BrotliResult::ResultSuccess;
        }
        let num_bytes_to_copy = core::cmp::min(num_bytes_left_in_cmd, copy.distance);
        let copy_count = self.copy_some_decoded_from_ring_buffer_to_decoded(copy.distance,
                                                                            num_bytes_to_copy);
        self.input_sub_offset += copy_count as usize;
        // by taking the min of copy.distance and items to copy, we are nonoverlapping
        // this means we can use split_at_mut to cut the array into nonoverlapping segments
        if copy_count != num_bytes_left_in_cmd {
            return BrotliResult::NeedsMoreOutput;
        }
        self.input_sub_offset = 0; // we're done
        BrotliResult::ResultSuccess
    }
    fn parse_dictionary(&mut self, dict_cmd:&DictCommand) -> BrotliResult {
        // dictionary words are bounded in size: make sure there's enough room for the whole word
        if self.input_sub_offset != 0 {
            // error: dictionary should never allow for partial words, since they fit in a small amount of space
            return BrotliResult::ResultFailure;
        }
        let copy_len = dict_cmd.word_size as u32;
        let word_len_category_index = kBrotliDictionaryOffsetsByLength[copy_len as usize] as u32;
        let word_index = (dict_cmd.word_id * copy_len) + word_len_category_index;
        let dict = &kBrotliDictionary;
        let word = &dict[(word_index as usize)..(word_index as usize + copy_len as usize)];
        let mut transformed_word = [0u8;kBrotliMaxDictionaryWordLength as usize + 13];
        let final_len = TransformDictionaryWord(&mut transformed_word[..],
                                                &word[..],
                                                copy_len as i32,
                                                dict_cmd.transform as i32);
        if self.decode_space_left_in_ring_buffer() < final_len as u32 {
            return BrotliResult::NeedsMoreOutput;
        }
        if dict_cmd.final_size != 0 && final_len as usize != dict_cmd.final_size as usize {
            return BrotliResult::ResultFailure;
        }
        if self.copy_to_ring_buffer(transformed_word.split_at(final_len as usize).0) as i32 != final_len {
            panic!("We already assured sufficient space in buffer for word: internal error");
        }
        BrotliResult::ResultSuccess
    }
    fn parse_command<SliceType:alloc::SliceWrapper<u8>>(&mut self, cmd: &Command<SliceType>) -> BrotliResult {
        match cmd {
              &Command::Copy(ref copy) => self.parse_copy(copy),
              &Command::Dict(ref dict) => self.parse_dictionary(dict),
              &Command::Literal(ref literal) => self.parse_literal(literal),
        }
    }
    pub fn recode<SliceType:alloc::SliceWrapper<u8>>(&mut self,
                  input:&[Command<SliceType>],
                  input_offset : &mut usize,
                  output :&mut[u8],
                  output_offset: &mut usize) -> BrotliResult {
        if *input_offset > input.len() {
            return BrotliResult::ResultFailure;
        }
        for cmd in input.split_at(*input_offset).1.iter() {
            loop {
                let mut res = self.flush(output, output_offset);
                 match res {
                    BrotliResult::ResultSuccess => {},
                    _ => {return res}
                 }
                 res = self.parse_command(cmd);
                 match res {
                    BrotliResult::ResultSuccess => {
                        assert_eq!(self.input_sub_offset, 0); // done w/this command, no partial work
                        break;
                    }, // move on to the next command
                    BrotliResult::NeedsMoreOutput => continue, // flush, and try again
                    _ => return res,
                 }
            }
            *input_offset += 1;
        }
        self.flush(output, output_offset)
    }
}
