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
pub struct EncoderSpecialization {
    backing: [u8; 128],
    max_size: usize,
}
impl EncoderSpecialization {
    pub fn new() -> Self {
        EncoderSpecialization{
            backing:[0;128],
            max_size: 0usize,
        }
    }
}
impl Default for EncoderSpecialization {
    fn default() -> Self {
        Self::new()
    }
}

impl EncoderOrDecoderSpecialization for EncoderSpecialization {
    const DOES_CALLER_WANT_ORIGINAL_FILE_BYTES: bool = false;
    const IS_DECODING_FILE: bool = false;
    fn alloc_literal_buffer<AllocU8:Allocator<u8>>(&mut self,
                                                   m8:&mut AllocU8,
                                                   len: usize) -> AllocatedMemoryPrefix<u8, AllocU8> {
        if len > self.max_size {
            self.max_size = len;
        }
        AllocatedMemoryPrefix::<u8, AllocU8>::new(m8, self.max_size)
    }
    fn get_input_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                      data:&'a [Command<ISlice>],
                                                      offset: usize,
                                                      _backing:&'a Command<ISlice>) -> &'a Command<ISlice> {
        &data[offset]
    }
    fn get_output_command<'a, AllocU8:Allocator<u8>>(&self,
                                                     _data:&'a mut [Command<AllocatedMemoryPrefix<u8, AllocU8>>],
                                                     _offset: usize,
                                                     backing:&'a mut Command<AllocatedMemoryPrefix<u8, AllocU8>>) -> &'a mut Command<AllocatedMemoryPrefix<u8, AllocU8>> {
        backing
    }
    fn get_source_copy_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                            data: &'a Command<ISlice>,
                                                            backing: &'a CopyCommand) -> &'a CopyCommand {
        match *data {
            Command::Copy(ref cc) => cc,
            _ => backing,
        }
    }
    fn get_source_literal_command<'a,
                                  ISlice:SliceWrapper<u8>
                                         +Default>(&self,
                                                   data: &'a Command<ISlice>,
                                                   backing: &'a LiteralCommand<ISlice>) -> &'a LiteralCommand<ISlice> {
        match *data {
            Command::Literal(ref lc) => lc,
            _ => backing,
        }        
    }
    fn get_source_dict_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                            data: &'a Command<ISlice>,
                                                            backing: &'a DictCommand) -> &'a DictCommand {
        match *data {
            Command::Dict(ref dc) => dc,
            _ => backing,
        }                
    }
    fn get_literal_byte<ISlice:SliceWrapper<u8>>(&self,
                                                   in_cmd: &LiteralCommand<ISlice>,
                                                   index: usize) -> u8 {
        in_cmd.data.slice()[index]
    }
    fn get_recoder_output<'a>(&'a mut self,
                              _passed_in_output_bytes: &'a mut [u8]) -> &'a mut[u8] {
        assert_eq!(Self::DOES_CALLER_WANT_ORIGINAL_FILE_BYTES, false);
        &mut self.backing[..]
    }
    fn get_recoder_output_offset<'a>(&self,
                                     _passed_in_output_bytes: &'a mut usize,
                                     backing: &'a mut usize) -> &'a mut usize {
        assert_eq!(Self::DOES_CALLER_WANT_ORIGINAL_FILE_BYTES, false);
        //*backing = self.backing.len();
        backing
    }
                          

}
