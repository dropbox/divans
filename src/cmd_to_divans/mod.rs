use alloc::{SliceWrapper, Allocator};

use codec::EncoderOrDecoderSpecialization;

use super::interface::{CopyCommand,DictCommand,LiteralCommand,Command};

use codec::AllocatedMemoryPrefix;
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

impl EncoderOrDecoderSpecialization for EncoderSpecialization {
    fn does_caller_want_original_file_bytes(&self) -> bool {
        false
    }
    fn alloc_literal_buffer<AllocU8:Allocator<u8>>(&mut self,
                                                   m8:&mut AllocU8,
                                                   len: usize) -> AllocatedMemoryPrefix<AllocU8> {
        if len > self.max_size {
            self.max_size = len;
        }
        AllocatedMemoryPrefix::<AllocU8>::new(m8, self.max_size)
    }
    fn get_input_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                      data:&'a [Command<ISlice>],
                                                      offset: usize,
                                                      _backing:&'a Command<ISlice>) -> &'a Command<ISlice> {
        &data[offset]
    }
    fn get_output_command<'a, AllocU8:Allocator<u8>>(&self,
                                                     _data:&'a mut [Command<AllocatedMemoryPrefix<AllocU8>>],
                                                     _offset: usize,
                                                     backing:&'a mut Command<AllocatedMemoryPrefix<AllocU8>>) -> &'a mut Command<AllocatedMemoryPrefix<AllocU8>> {
        backing
    }
    fn get_source_copy_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                            data: &'a Command<ISlice>,
                                                            backing: &'a CopyCommand) -> &'a CopyCommand {
        match data {
            &Command::Copy(ref cc) => cc,
            _ => backing,
        }
    }
    fn get_source_literal_command<'a,
                                  ISlice:SliceWrapper<u8>
                                         +Default>(&self,
                                                   data: &'a Command<ISlice>,
                                                   backing: &'a LiteralCommand<ISlice>) -> &'a LiteralCommand<ISlice> {
        match data {
            &Command::Literal(ref lc) => lc,
            _ => backing,
        }        
    }
    fn get_source_dict_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                            data: &'a Command<ISlice>,
                                                            backing: &'a DictCommand) -> &'a DictCommand {
        match data {
            &Command::Dict(ref dc) => dc,
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
        assert_eq!(self.does_caller_want_original_file_bytes(), false);
        &mut self.backing[..]
    }
    fn get_recoder_output_offset<'a>(&self,
                                     _passed_in_output_bytes: &'a mut usize,
                                     backing: &'a mut usize) -> &'a mut usize {
        assert_eq!(self.does_caller_want_original_file_bytes(), false);
        //*backing = self.backing.len();
        backing
    }
                          

}
