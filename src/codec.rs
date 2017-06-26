#![allow(dead_code)]
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
//    Decoder,
//    Recoder,
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

impl<AllocU8:Allocator<u8>> Default for AllocatedMemoryPrefix<AllocU8> {
    fn default() -> Self {
        AllocatedMemoryPrefix(AllocU8::AllocatedMemory::default(), 0usize)
    }        
}
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
#[derive(Copy, Clone)]
enum CopySubstate {
     Begin,
     DistanceLengthGreater15Less25, // length not between 1 and 15, inclusive.. second nibble results in 15-24
     DistanceMantissaNibbles(u8, u32), // nibble count (up to 6), intermediate result
     DistanceDecoded,
     CountLengthFirstGreater14Less25, // length not between 0 and 14 inclusive... second nibble results in 15-24
     CountMantissaNibbles(u8, u32), //nibble count, intermediate result
     FullyDecoded
}
struct CopyState {
   cc:CopyCommand,
   state: CopySubstate,
}

impl CopyState {
    fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                        Specialization:EncoderOrDecoderSpecialization,
                        AllocU8:Allocator<u8>>(&mut self,
                                               superstate: &mut CrossCommandState<ArithmeticCoder,
                                                                                  Specialization,
                                                                                  AllocU8>,
                                               in_cmd: &CopyCommand,
                                               input_bytes:&[u8],
                                                    input_offset: &mut usize,
                                                    output_bytes:&mut [u8],
                                                    output_offset: &mut usize) -> BrotliResult {
        let dlen: u8 = (core::mem::size_of_val(&in_cmd.distance) as u32 * 8 - in_cmd.distance.leading_zeros()) as u8;
        if dlen ==0 {
            return BrotliResult::ResultFailure; // not allowed to copy from 0 distance
        }
        let uniform_prob = CDF16::<FrequentistCDFUpdater>::default();
        loop {
            superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset);
            match self.state {
                CopySubstate::Begin => {
                    let mut beg_nib = core::cmp::min(15, dlen - 1);
                    superstate.coder.get_or_put_nibble(&mut beg_nib, &uniform_prob);
                    if beg_nib == 15 {
                        self.state = CopySubstate::DistanceLengthGreater15Less25;
                    } else if beg_nib == 0 {
                        self.cc.distance = 1;
                        self.state = CopySubstate::DistanceDecoded;
                    } else {
                        self.state = CopySubstate::DistanceMantissaNibbles(beg_nib,  1 << (beg_nib + 1));
                    }
                },
                CopySubstate::DistanceLengthGreater15Less25 => {
                    let mut last_nib = dlen - 15;
                    superstate.coder.get_or_put_nibble(&mut last_nib, &uniform_prob);
                    self.state = CopySubstate::DistanceMantissaNibbles(last_nib + 15,  1 << (last_nib + 16));
                },
                CopySubstate::DistanceMantissaNibbles(len_remaining, decoded_so_far) => {
                    panic!("unimpl");
                },
                _ => panic!("unimpl"),
            }
        }
    }
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

#[derive(Copy, Clone)]
enum DictSubstate {
    Begin,
    WordSizeGreater18Less25, // if in this state, second nibble results in values 19-24 (first nibble was between 4 and 18)
    WordSizeDecoded,
    WordIndexMantissa(u8, u32), // assume the length is < (1 << WordSize), decode that many nibbles and use binary encoding
    TransformHigh, // total number of transforms <= 121 therefore; nibble must be < 8
    TransformLow,
    FullyDecoded,
}
struct DictState {
   dc:DictCommand,
   state: DictSubstate,
}
impl DictState {
    fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                             Specialization:EncoderOrDecoderSpecialization,
                             AllocU8:Allocator<u8>>(&mut self,
                                                    _state: &mut CrossCommandState<ArithmeticCoder,
                                                                             Specialization,
                                                                                   AllocU8>,
                                                    in_cmd: &DictCommand,
                                                    _input_bytes:&[u8],
                                                    _input_offset: &mut usize,
                                                    _output_bytes:&mut [u8],
                                                    _output_offset: &mut usize) -> BrotliResult {
        panic!("unimpl");
    }
}

#[derive(Copy, Clone)]
enum LiteralSubstate {
    Begin,
    LiteralCountLengthGreater14Less25,
    LiteralCountMantissaNibbles(u8, u32),
    LiteralNibbleIndex(u32)
}
struct LiteralState<AllocU8:Allocator<u8>> {
   lc:LiteralCommand<AllocatedMemoryPrefix<AllocU8>>,
   state: LiteralSubstate,
}

impl<AllocU8:Allocator<u8>> LiteralState<AllocU8> {
    fn encode_or_decode<ISlice: SliceWrapper<u8>,
                        ArithmeticCoder:ArithmeticEncoderOrDecoder,
                        Specialization:EncoderOrDecoderSpecialization
                        >(&mut self,
                          _state: &mut CrossCommandState<ArithmeticCoder,
                                                         Specialization,
                                                         AllocU8>,
                          in_cmd: &LiteralCommand<ISlice>,
                          _input_bytes:&[u8],
                          _input_offset: &mut usize,
                          _output_bytes:&mut [u8],
                          _output_offset: &mut usize) -> BrotliResult {
        panic!("unimpl");
    }
}



enum EncodeOrDecodeState<AllocU8: Allocator<u8> > {
    Begin,
    Literal(LiteralState<AllocU8>),
    Dict(DictState),
    Copy(CopyState),
    PopulateRingBuffer(usize),
    DivansSuccess,
}

impl<AllocU8:Allocator<u8>> Default for EncodeOrDecodeState<AllocU8> {
    fn default() -> Self {
        EncodeOrDecodeState::Begin
    }
}

pub struct CrossCommandState<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                             Specialization:EncoderOrDecoderSpecialization,
                             AllocU8:Allocator<u8>> {
    coder: ArithmeticCoder,
    specialization: Specialization,
    _phantom: core::marker::PhantomData<AllocU8>
}
pub struct DivansCodec<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                       Specialization:EncoderOrDecoderSpecialization,
                       AllocU8: Allocator<u8>> {
    cross_command_state: CrossCommandState<ArithmeticCoder,
                                           Specialization,
                                           AllocU8>,
    m8: AllocU8,
    state : EncodeOrDecodeState<AllocU8>,
    // this holds recent Command::LiteralCommand's buffers when
    // those commands are repurposed for other things like LiteralCommand
    literal_cache: [AllocU8::AllocatedMemory; CMD_BUFFER_SIZE],
    // need state variable describing the item we are building
}

pub enum OneCommandReturn {
    Advance,
    BufferExhausted(BrotliResult),
}

impl<ArithmeticCoder:ArithmeticEncoderOrDecoder,
     Specialization: EncoderOrDecoderSpecialization,
     AllocU8: Allocator<u8>> DivansCodec<ArithmeticCoder, Specialization, AllocU8> {
    pub fn specialization(&mut self) -> &mut Specialization{
        &mut self.cross_command_state.specialization
    }
    pub fn coder(&mut self) -> &mut ArithmeticCoder {
        &mut self.cross_command_state.coder
    }
    pub fn encode_or_decode<ISl:SliceWrapper<u8>>(&mut self,
                                                  input_bytes: &[u8],
                                                  input_bytes_offset: &mut usize,
                                                  output_bytes: &mut [u8],
                                                  output_bytes_offset: &mut usize,
                                                  input_commands: &[Command<ISl>],
                                                  input_command_offset: &mut usize,
                                                  output_commands: &mut[Command<AllocatedMemoryPrefix<AllocU8>>],
                                                  output_command_offset: &mut usize) -> BrotliResult {
        loop {
            let i_cmd_backing = Command::<ISl>::nop();
            let mut o_cmd_backing = Command::<AllocatedMemoryPrefix<AllocU8>>::nop();
            let output_commands_len = output_commands.len();
            let in_cmd = self.cross_command_state.specialization.get_input_command(input_commands,
                                                                                   *input_command_offset,
                                                                                   &i_cmd_backing);
            let mut o_cmd = self.cross_command_state.specialization.get_output_command(output_commands,
                                                                                       *output_command_offset,
                                                                                       &mut o_cmd_backing);
            match self.encode_or_decode_one_command(input_bytes,
                                                    input_bytes_offset,
                                                    output_bytes,
                                                    output_bytes_offset,
                                                    in_cmd,
                                                    o_cmd) {
                OneCommandReturn::Advance => {
                    *input_command_offset += 1;
                    *output_command_offset += 1;
                    if input_commands.len() == *input_command_offset {
                        return BrotliResult::NeedsMoreInput;
                    }
                    if output_commands_len == *output_command_offset {
                        return BrotliResult::NeedsMoreOutput;
                    }
                },
                OneCommandReturn::BufferExhausted(result) => {
                    return result;
                }
            }
        }
    }
    pub fn encode_or_decode_one_command<ISl:SliceWrapper<u8>>(&mut self,
                                                  input_bytes: &[u8],
                                                  input_bytes_offset: &mut usize,
                                                  output_bytes: &mut [u8],
                                                  output_bytes_offset: &mut usize,
                                                  input_cmd: &Command<ISl>,
                                                  o_cmd: &mut Command<AllocatedMemoryPrefix<AllocU8>>,
                                                  ) -> OneCommandReturn {
        let half = 128u8;
        loop {
            let mut new_state: Option<EncodeOrDecodeState<AllocU8>>;
            match &mut self.state {
                &mut EncodeOrDecodeState::DivansSuccess => {
                    return OneCommandReturn::BufferExhausted(BrotliResult::ResultSuccess);
                },
                &mut EncodeOrDecodeState::Begin => {
                    let mut is_copy = false;
                    let mut is_dict_or_end = false;
                    let mut is_end = false;
                    match input_cmd {
                        &Command::Copy(_) => is_copy = true,
                        &Command::Dict(_) => is_dict_or_end = true,
                        _ => {},
                    }
                    self.cross_command_state.coder.get_or_put_bit(&mut is_copy, half);
                    if is_copy == false {
                        self.cross_command_state.coder.get_or_put_bit(&mut is_dict_or_end, half);
                        if is_dict_or_end == true {
                            self.cross_command_state.coder.get_or_put_bit(&mut is_end, half);
                            new_state = Some(EncodeOrDecodeState::Dict(DictState {
                                dc: DictCommand::nop(),
                                state: DictSubstate::Begin,
                            }));
                        } else {
                            new_state = Some(EncodeOrDecodeState::Literal(LiteralState {
                                lc:LiteralCommand::<AllocatedMemoryPrefix<AllocU8>>{
                                    data:AllocatedMemoryPrefix::default(),
                                },
                                state:LiteralSubstate::Begin,
                            }));
                        }
                    } else {
                        new_state = Some(EncodeOrDecodeState::Copy(CopyState {
                            cc: CopyCommand {
                                distance:0,
                                num_bytes:0,
                            },
                            state:CopySubstate::Begin,
                        }));
                    }
                    if is_end {
                        new_state = Some(EncodeOrDecodeState::DivansSuccess);
                    }
                }
                &mut EncodeOrDecodeState::Copy(ref mut copy_state) => {
                    match copy_state.
                        encode_or_decode(&mut self.cross_command_state,
                                                      match input_cmd {
                                                          &Command::Copy(ref in_copy_state) => in_copy_state,
                                                          _ => {
                                                              // unreachable unless caller passed in different values for input_cmd
                                                              // on subsequent calls that didn't return Advance
                                                              return OneCommandReturn::BufferExhausted(BrotliResult::ResultFailure);
                                                          }
                                                      },
                                                      input_bytes,
                                                      input_bytes_offset,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        BrotliResult::ResultSuccess => {
                            *o_cmd = Command::Copy(core::mem::replace(&mut copy_state.cc, CopyCommand::nop()));
                            new_state = Some(EncodeOrDecodeState::PopulateRingBuffer(0));
                        },
                        retval @ _ => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                }
                &mut EncodeOrDecodeState::Literal(ref mut lit_state) => {
                    match lit_state.encode_or_decode(&mut self.cross_command_state,
                                                      match input_cmd {
                                                          &Command::Literal(ref in_copy_state) => in_copy_state,
                                                          _ => {
                                                              // unreachable unless caller passed in different values for input_cmd
                                                              // on subsequent calls that didn't return Advance
                                                              return OneCommandReturn::BufferExhausted(BrotliResult::ResultFailure);
                                                          }
                                                      },
                                                      input_bytes,
                                                      input_bytes_offset,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        BrotliResult::ResultSuccess => {
                            *o_cmd = Command::Literal(core::mem::replace(&mut lit_state.lc,
                                                                         LiteralCommand::<AllocatedMemoryPrefix<AllocU8>>::nop()));
                            new_state = Some(EncodeOrDecodeState::PopulateRingBuffer(0));
                        },
                        retval @ _ => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                }
                &mut EncodeOrDecodeState::Dict(ref mut dict_state) => {
                    match dict_state.encode_or_decode(&mut self.cross_command_state,
                                                      match input_cmd {
                                                          &Command::Dict(ref in_dict_state) => in_dict_state,
                                                          _ => {
                                                              // unreachable unless caller passed in different values for input_cmd
                                                              // on subsequent calls that didn't return Advance
                                                              return OneCommandReturn::BufferExhausted(BrotliResult::ResultFailure);
                                                          }
                                                      },
                                                      input_bytes,
                                                      input_bytes_offset,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        BrotliResult::ResultSuccess => {
                            *o_cmd = Command::Dict(core::mem::replace(&mut dict_state.dc, DictCommand::nop()));
                            new_state = Some(EncodeOrDecodeState::PopulateRingBuffer(0));
                        },
                        retval @ _ => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                }
                _ =>{panic!("Unimpl");},
            }
            match new_state {
                Some(ns) => self.state = ns,
                None => {},
            }
        }
    }
                        
}
