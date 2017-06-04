extern crate core;
extern crate alloc_no_stdlib as alloc;
extern crate brotli_decompressor;
pub use brotli_decompressor::{BrotliResult};
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
use brotli_decompressor::dictionary::{kBrotliMaxDictionaryWordLength, kBrotliDictionary,
                                      kBrotliDictionaryOffsetsByLength};
use brotli_decompressor::transform::{TransformDictionaryWord};

#[derive(Debug)]
pub struct CopyCommand {
    pub distance: usize,
    pub num_bytes: usize,
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

pub struct DivansRecodeState<RingBuffer: SliceWrapperMut<u8> + SliceWrapper<u8> + Default>{
    input_sub_offset :usize,
    ring_buffer: RingBuffer,
    ring_buffer_decode_index: u32,
    ring_buffer_output_index: u32,
}
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
        if self.ring_buffer_output_index != self.ring_buffer.slice().len() as u32 {
            return BrotliResult::NeedsMoreOutput;
        }
        BrotliResult::ResultSuccess
    }
    fn decode_space_left_in_ring_buffer(&self) -> usize {
        if self.ring_buffer_output_index < self.ring_buffer_decode_index {
            return self.ring_buffer_output_index as usize + self.ring_buffer.slice().len() - 1 - self.ring_buffer_decode_index as usize;
        }
        return self.ring_buffer_output_index as usize - 1 - self.ring_buffer_decode_index as usize;
    }

    // takes in a buffer of data to copy to the ring buffer--returns the number of bytes persisted
    fn copy_to_ring_buffer(&mut self, mut data: &[u8]) -> usize {
        data = data.split_at(core::cmp::min(data.len(), self.decode_space_left_in_ring_buffer())).0;
        let mut retval = 0usize;
        let first_section = self.ring_buffer.slice_mut().len() - self.ring_buffer_decode_index as usize;
        let amount_to_copy = core::cmp::min(data.len(), first_section);
        let (data_first, data_second) = data.split_at(amount_to_copy);
        self.ring_buffer.slice_mut()[self.ring_buffer_decode_index as usize .. (self.ring_buffer_decode_index as usize + amount_to_copy)].clone_from_slice(data_first);
        self.ring_buffer_decode_index += amount_to_copy as u32;
        retval += amount_to_copy;
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
          self.input_sub_offset += bytes_copied;
          return BrotliResult::NeedsMoreOutput;
       }
       self.input_sub_offset = 0;
       BrotliResult::ResultSuccess
    }
    fn parse_copy(&mut self, copy:&CopyCommand) -> BrotliResult {
        panic!("unimplemented");
        BrotliResult::ResultSuccess
    }
    fn parse_dictionary(&mut self, dict_cmd:&DictCommand) -> BrotliResult {
        // dictionary words are bounded in size: make sure there's enough room for the whole word
        if self.input_sub_offset != 0 {
            // error: dictionary should never allow for partial words, since they fit in a small amount of space
            return BrotliResult::ResultFailure;
        }
        if self.decode_space_left_in_ring_buffer() < kBrotliMaxDictionaryWordLength as usize + 13 {
            return BrotliResult::NeedsMoreOutput;
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
    pub fn encode<SliceType:alloc::SliceWrapper<u8>>(&mut self,
                  input:&[&Command<SliceType>],
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
                    BrotliResult::ResultSuccess => break, // move on to the next command
                    BrotliResult::NeedsMoreOutput => continue, // flush, and try again
                    _ => return res,
                 }
            }
            *input_offset += 1;
        }
        self.flush(output, output_offset)
    }
}
