use alloc::{SliceWrapper, Allocator};

use codec::EncoderOrDecoderSpecialization;

use super::interface::{CopyCommand,DictCommand,LiteralCommand,Command};

use codec::AllocatedMemoryPrefix;
struct DecoderSpecialization {
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
    fn alloc_literal_buffer<AllocU8:Allocator<u8>>(&mut self,
                                                   m8:&mut AllocU8,
                                                   len: usize) -> AllocatedMemoryPrefix<AllocU8> {
        if len > self.max_size {
            self.max_size = len;
        }
        AllocatedMemoryPrefix::<AllocU8>::new(m8, self.max_size)
    }
    fn get_input_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                      _data:&'a [Command<ISlice>],
                                                      _offset: usize,
                                                      backing:&'a Command<ISlice>) -> &'a Command<ISlice> {
        backing
    }
    fn get_output_command<'a, AllocU8:Allocator<u8>>(&self, data:&'a mut [Command<AllocatedMemoryPrefix<AllocU8>>],
                                                     offset: usize,
                                                     _backing:&'a mut Command<AllocatedMemoryPrefix<AllocU8>>) -> &'a mut Command<AllocatedMemoryPrefix<AllocU8>> {
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
    fn get_literal_nibble<ISlice:SliceWrapper<u8>>(&self,
                                                   in_cmd: &LiteralCommand<ISlice>,
                                                   index: usize) -> u8 {
        in_cmd.data.slice()[index]
    }
    fn get_recoder_output<'a>(&'a mut self,
                              passed_in_output_bytes: &'a mut [u8]) -> &'a mut[u8] {
        passed_in_output_bytes
    }
    fn get_recoder_output_offset<'a>(&self,
                                     passed_in_output_bytes: &'a mut usize,
                                     _backing: &'a mut usize) -> &'a mut usize {
        passed_in_output_bytes
    }
                          

}
