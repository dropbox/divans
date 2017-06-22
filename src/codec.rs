use core;
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use brotli_decompressor::BrotliResult;
pub const CMD_BUFFER_SIZE: usize = 16;
use super::probability::{CDF16, FrequentistCDFUpdater};
use super::interface::{
    CopyCommand,
    DictCommand,
    LiteralCommand,
    Command,
    Decoder,
    Recoder,
    ArithmeticEncoderOrDecoder
};

pub trait EncoderOrDecoderSpecialization {
    fn alloc_literal_buffer<AllocU8: Allocator<u8>>(&self, len: usize) -> AllocU8::AllocatedMemory;
    fn get_input_command<'a, ISlice:SliceWrapper<u8>>(&self, data:&'a [Command<ISlice>],offset: usize, backing:&'a Command<ISlice>) -> &'a Command<ISlice>;
    fn get_output_command<'a, AllocU8:Allocator<u8>>(&self, data:&'a mut [Command<AllocatedMemoryPrefix<AllocU8>>],
                                                    offset: usize,
                                                    backing:&'a mut Command<AllocatedMemoryPrefix<AllocU8>>) -> &'a mut Command<AllocatedMemoryPrefix<AllocU8>>;
}


pub struct AllocatedMemoryPrefix<AllocU8:Allocator<u8>>(AllocU8::AllocatedMemory, usize);
impl<AllocU8:Allocator<u8>> AllocatedMemoryPrefix<AllocU8> {
    fn replace_with_empty(&mut self) ->AllocU8::AllocatedMemory {
        core::mem::replace(&mut self.0, AllocU8::AllocatedMemory::default())
    }
}

impl<AllocU8:Allocator<u8>> SliceWrapperMut<u8> for AllocatedMemoryPrefix<AllocU8> {
    fn slice_mut(&mut self) -> &mut [u8] {
        self.0.slice_mut().split_at_mut(self.1).0
    }
}
impl<AllocU8:Allocator<u8>> SliceWrapper<u8> for AllocatedMemoryPrefix<AllocU8> {
    fn slice(&self) -> &[u8] {
        self.0.slice().split_at(self.1).0
    }
}

pub struct DivansCodec<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                       Specialization:EncoderOrDecoderSpecialization,
                       AllocU8: Allocator<u8>> {
    coder: ArithmeticCoder,
    specialization: Specialization,
    m8: AllocU8,
    // this holds recent Command::LiteralCommand's buffers when
    // those commands are repurposed for other things like LiteralCommand
    literal_cache: [AllocU8::AllocatedMemory; CMD_BUFFER_SIZE],
    // need state variable describing the item we are building
}

enum CopySubstate {
     DistanceLength(u8), // length so far
     DistanceMantissa(u8), // current lsb
     CountLength(u8), // length so far
     CountMantissa(u8), // current lsb (should we use unary here?)
     FullyDecoded
}
struct CopyState {
   cc:CopyCommand, 
   state: CopySubstate,
}

impl<AllocU8:Allocator<u8>> From<CopyState> for Command<AllocatedMemoryPrefix<AllocU8>> {
     fn from(cp: CopyState) -> Self {
        Command::Copy(cp.cc)
     }
}
impl<AllocU8:Allocator<u8>> From<DictState> for Command<AllocatedMemoryPrefix<AllocU8>> {
     fn from(dd: DictState) -> Self {
        Command::Dict(dd.dc)
     }
}
impl<AllocU8:Allocator<u8>> From<LiteralState<AllocU8>> for Command<AllocatedMemoryPrefix<AllocU8>> {
     fn from(ll: LiteralState<AllocU8>) -> Self {
        Command::Literal(ll.lc)
     }
}
enum DictSubstate {
     WordSize(u8),
     WordIndexLength(u8),
     WordIndexMantissa(u8),
     TransformA, // materialized as a single nibble
     TransformB,
     FullyDecoded,
}
struct DictState {
   dc:DictCommand,
   state: DictSubstate,
}

struct LiteralSubstate {
       length: u8
}
struct LiteralState<AllocU8:Allocator<u8>> {
   lc:LiteralCommand<AllocatedMemoryPrefix<AllocU8>>,
   
}

impl<ArithmeticCoder:ArithmeticEncoderOrDecoder,
     Specialization: EncoderOrDecoderSpecialization,
     AllocU8: Allocator<u8>> DivansCodec<ArithmeticCoder, Specialization, AllocU8> {
    pub fn encode_or_decode<ISl:SliceWrapper<u8>>(&mut self,
                                                  input_bytes: &[u8],
                                                  input_bytes_offset: &mut usize,
                                                  output_bytes: &mut [u8],
                                                  output_bytes_offset: &mut usize,
                                                  input_commands: &[Command<ISl>],
                                                  input_command_offset: &mut usize,
                                                  output_commands: &mut[Command<AllocatedMemoryPrefix<AllocU8>>],
                                                  output_command_offset: &mut usize) -> BrotliResult {
        let uniform_prob = CDF16::<FrequentistCDFUpdater>::default();
        let half = 128u8;
        loop {
            let cur_backing = Command::<ISl>::nop();
            let cur_cmd = self.specialization.get_input_command(input_commands, *input_command_offset, &cur_backing);
            let mut is_copy = false;
            let mut is_dict_or_end = false;
            let mut is_end = false;
            match cur_cmd {
                &Command::Copy(_) => is_copy = true,
                &Command::Dict(_) => is_dict_or_end = true,
                _ => {},
            }
            self.coder.get_or_put_bit(&mut is_copy, half);
            let mut cmd_backing = Command::<AllocatedMemoryPrefix<AllocU8>>::nop();
            let mut cur_command = self.specialization.get_output_command(output_commands, *output_command_offset, &mut cmd_backing);
            if is_copy == false {
                self.coder.get_or_put_bit(&mut is_dict_or_end, half);
                if is_dict_or_end == true {
                    self.coder.get_or_put_bit(&mut is_end, half);
                } else {
                    //cur_command = Command::<AllocatedMemoryPrefix<AllocU8>::LiteralCommand
                }
            }
            if is_end {
                return BrotliResult::ResultSuccess;
            }
            
        }
        BrotliResult::ResultFailure
    }
                        
}
