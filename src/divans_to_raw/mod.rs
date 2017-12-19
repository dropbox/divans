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

use alloc::{SliceWrapper, Allocator};

use codec::EncoderOrDecoderSpecialization;

use super::interface::{CopyCommand,DictCommand,LiteralCommand,Command};

use slice_util::AllocatedMemoryPrefix;

#[derive(Default)]
pub struct DecoderSpecialization {
    max_size: usize,    
}

impl DecoderSpecialization {
    pub fn new() -> Self {
        DecoderSpecialization{
            max_size:0usize,
        }
    }
}



impl EncoderOrDecoderSpecialization for DecoderSpecialization {
    const DOES_CALLER_WANT_ORIGINAL_FILE_BYTES: bool = true;
    const IS_DECODING_FILE: bool = true;
    fn alloc_literal_buffer<AllocU8:Allocator<u8>>(&mut self,
                                                   m8:&mut AllocU8,
                                                   len: usize) -> AllocatedMemoryPrefix<u8, AllocU8> {
        if len > self.max_size {
            self.max_size = len;
        }
        AllocatedMemoryPrefix::<u8, AllocU8>::new(m8, self.max_size)
    }
    fn get_input_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                      _data:&'a [Command<ISlice>],
                                                      _offset: usize,
                                                      backing:&'a Command<ISlice>) -> &'a Command<ISlice> {
        backing
    }
    fn get_output_command<'a, AllocU8:Allocator<u8>>(&self, data:&'a mut [Command<AllocatedMemoryPrefix<u8, AllocU8>>],
                                                     offset: usize,
                                                     _backing:&'a mut Command<AllocatedMemoryPrefix<u8, AllocU8>>) -> &'a mut Command<AllocatedMemoryPrefix<u8, AllocU8>> {
        &mut data[offset]
    }
    fn get_source_copy_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                            _data: &'a Command<ISlice>,
                                                            backing: &'a CopyCommand) -> &'a CopyCommand {
        backing
    }
    fn get_source_literal_command<'a,
                                  ISlice:SliceWrapper<u8>
                                         +Default>(&self,
                                                   _data: &'a Command<ISlice>,
                                                   backing: &'a LiteralCommand<ISlice>) -> &'a LiteralCommand<ISlice> {
        backing
    }
    fn get_source_dict_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                            _data: &'a Command<ISlice>,
                                                            backing: &'a DictCommand) -> &'a DictCommand {
        backing
    }
    #[inline(always)]
    fn get_literal_byte<ISlice:SliceWrapper<u8>>(&self,
                                                   _in_cmd: &LiteralCommand<ISlice>,
                                                   _index: usize) -> u8 {
        0
    }
    fn get_recoder_output<'a>(&'a mut self,
                              passed_in_output_bytes: &'a mut [u8]) -> &'a mut[u8] {
        assert_eq!(Self::DOES_CALLER_WANT_ORIGINAL_FILE_BYTES, true);
        passed_in_output_bytes
    }
    fn get_recoder_output_offset<'a>(&self,
                                     passed_in_output_bytes: &'a mut usize,
                                     _backing: &'a mut usize) -> &'a mut usize {
        assert_eq!(Self::DOES_CALLER_WANT_ORIGINAL_FILE_BYTES, true);
        passed_in_output_bytes
    }
                          

}
