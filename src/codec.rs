#![allow(dead_code)]
use core;
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use brotli_decompressor::dictionary::{kBrotliMaxDictionaryWordLength, kBrotliDictionary};
use brotli_decompressor::BrotliResult;
pub const CMD_BUFFER_SIZE: usize = 16;
use brotli_decompressor::transform::{TransformDictionaryWord};
use interface::Recoder;
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
    fn get_source_copy_command<'a, ISlice:SliceWrapper<u8>>(&self, &'a Command<ISlice>, &'a CopyCommand) -> &'a CopyCommand;
    fn get_source_literal_command<'a, ISlice:SliceWrapper<u8>+Default>(&self, &'a Command<ISlice>, &'a LiteralCommand<ISlice>) -> &'a LiteralCommand<ISlice>;
    fn get_source_dict_command<'a, ISlice:SliceWrapper<u8>>(&self, &'a Command<ISlice>, &'a DictCommand) -> &'a DictCommand;
    fn get_literal_nibble<ISlice:SliceWrapper<u8>>(&self,
                                                   in_cmd: &LiteralCommand<ISlice>,
                                                   index: usize) -> u8;
    fn get_recoder_output<'a>(&self, passed_in_output_bytes: &'a mut [u8]) -> &'a mut[u8];
    fn get_recoder_output_offset<'a>(&self, passed_in_output_bytes: &'a mut usize) -> &'a mut usize;
                          
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

fn round_up_mod_4(val: u8) -> u8 {
    ((val - 1)|3)+1
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
        let clen: u8 = (core::mem::size_of_val(&in_cmd.num_bytes) as u32 * 8 - in_cmd.num_bytes.leading_zeros()) as u8;
        if dlen ==0 {
            return BrotliResult::ResultFailure; // not allowed to copy from 0 distance
        }
        let uniform_prob = CDF16::<FrequentistCDFUpdater>::default();
        loop {
            match superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset) {
                BrotliResult::ResultSuccess => {},
                need_something => return need_something,
            }
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
                        self.state = CopySubstate::DistanceMantissaNibbles(round_up_mod_4(beg_nib),  1 << (beg_nib + 1));
                    }
                },
                CopySubstate::DistanceLengthGreater15Less25 => {
                    let mut last_nib = dlen - 16;
                    superstate.coder.get_or_put_nibble(&mut last_nib, &uniform_prob);
                    self.state = CopySubstate::DistanceMantissaNibbles(round_up_mod_4(last_nib + 15),  1 << (last_nib + 16));
                },
                CopySubstate::DistanceMantissaNibbles(len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = (in_cmd.distance ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    superstate.coder.get_or_put_nibble(&mut last_nib, &uniform_prob);
                    let next_decoded_so_far = decoded_so_far | ((last_nib as u32) << next_len_remaining);
                
                    if next_len_remaining == 0 {
                        self.cc.distance = next_decoded_so_far;
                        self.state = CopySubstate::DistanceDecoded;
                    } else {
                        self.state  = CopySubstate::DistanceMantissaNibbles(
                            next_len_remaining,
                            next_decoded_so_far);
                    }
                },
                CopySubstate::DistanceDecoded => {
                    let mut beg_nib = core::cmp::min(15, clen);
                    superstate.coder.get_or_put_nibble(&mut beg_nib, &uniform_prob);
                    if beg_nib == 15 {
                        self.state = CopySubstate::CountLengthFirstGreater14Less25;
                    } else if beg_nib <= 1 {
                        self.cc.num_bytes = beg_nib as u32;
                        self.state = CopySubstate::FullyDecoded;
                    } else {
                        self.state = CopySubstate::CountMantissaNibbles(round_up_mod_4(beg_nib - 1),  1 << beg_nib);
                    }
                    
                }
                CopySubstate::CountLengthFirstGreater14Less25 => {
                    let mut last_nib = clen - 15;
                    superstate.coder.get_or_put_nibble(&mut last_nib, &uniform_prob);
                    self.state = CopySubstate::CountMantissaNibbles(round_up_mod_4(last_nib + 14),  1 << (last_nib + 15));
                },
                CopySubstate::CountMantissaNibbles(len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = (in_cmd.num_bytes ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    superstate.coder.get_or_put_nibble(&mut last_nib, &uniform_prob);
                    let next_decoded_so_far = decoded_so_far | ((last_nib as u32) << next_len_remaining);
                
                    if next_len_remaining == 0 {
                        self.cc.num_bytes = next_decoded_so_far;
                        self.state = CopySubstate::FullyDecoded;
                    } else {
                        self.state  = CopySubstate::CountMantissaNibbles(
                            next_len_remaining,
                            next_decoded_so_far);
                    }
                },
                CopySubstate::FullyDecoded => {
                    return BrotliResult::ResultSuccess;
                }
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
                                                    superstate: &mut CrossCommandState<ArithmeticCoder,
                                                                             Specialization,
                                                                                   AllocU8>,
                                                    in_cmd: &DictCommand,
                                                    input_bytes:&[u8],
                                                    input_offset: &mut usize,
                                                    output_bytes:&mut [u8],
                                                    output_offset: &mut usize) -> BrotliResult {
        if in_cmd.word_size < 4 {
            return BrotliResult::ResultFailure; // FIXME: do we have the nop do the right thing here?
        }
        let uniform_prob = CDF16::<FrequentistCDFUpdater>::default();
        loop {
            match superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset) {
                BrotliResult::ResultSuccess => {},
                need_something => return need_something,
            }
            match self.state {
                DictSubstate::Begin => {
                    let mut beg_nib = core::cmp::min(15, in_cmd.word_size - 4);
                    superstate.coder.get_or_put_nibble(&mut beg_nib, &uniform_prob);
                    if beg_nib == 15 {
                        self.state = DictSubstate::WordSizeGreater18Less25;
                    } else {
                        self.dc.word_size = beg_nib + 4;
                        self.state = DictSubstate::WordIndexMantissa(round_up_mod_4(1 << self.dc.word_size), 0);
                    }
                }
                DictSubstate::WordSizeGreater18Less25 => {
                    let mut beg_nib = in_cmd.word_size - 19;
                    superstate.coder.get_or_put_nibble(&mut beg_nib, &uniform_prob);
                    self.dc.word_size = beg_nib + 19;
                    self.state = DictSubstate::WordIndexMantissa(round_up_mod_4(1 << self.dc.word_size), 0);
                }
                DictSubstate::WordIndexMantissa(len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = (in_cmd.word_id ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    superstate.coder.get_or_put_nibble(&mut last_nib, &uniform_prob);
                    let next_decoded_so_far = decoded_so_far | ((last_nib as u32) << next_len_remaining);
                    if next_len_remaining == 0 {
                        self.dc.word_id = next_decoded_so_far;
                        self.state = DictSubstate::TransformHigh;
                    } else {
                        self.state  = DictSubstate::WordIndexMantissa(
                            next_len_remaining,
                            next_decoded_so_far);
                    }
                },
                DictSubstate::TransformHigh => {
                    let mut high_nib = in_cmd.transform >> 4;
                    superstate.coder.get_or_put_nibble(&mut high_nib, &uniform_prob);
                    self.dc.transform = high_nib << 4;
                    self.state = DictSubstate::TransformLow;
                }
                DictSubstate::TransformLow => {
                    let mut low_nib = in_cmd.transform & 0xf;
                    superstate.coder.get_or_put_nibble(&mut low_nib, &uniform_prob);
                    self.dc.transform |= low_nib;
                    let dict = &kBrotliDictionary;
                    let word = &dict[(self.dc.word_id as usize)..(self.dc.word_id as usize + self.dc.word_size as usize)];
                    let mut transformed_word = [0u8;kBrotliMaxDictionaryWordLength as usize + 13];
                    let final_len = TransformDictionaryWord(&mut transformed_word[..],
                                                            &word[..],
                                                            self.dc.word_size as i32,
                                                            self.dc.transform as i32);
                    self.dc.final_size = final_len as u8;// WHA
                    self.state = DictSubstate::FullyDecoded;
                    return BrotliResult::ResultSuccess;
                }
                DictSubstate::FullyDecoded => {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
}

#[derive(Copy, Clone)]
enum LiteralSubstate {
    Begin,
    LiteralCountLengthGreater14Less25,
    LiteralCountMantissaNibbles(u8, u32),
    LiteralNibbleIndex(u32),
    FullyDecoded,
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
                          superstate: &mut CrossCommandState<ArithmeticCoder,
                                                         Specialization,
                                                         AllocU8>,
                          in_cmd: &LiteralCommand<ISlice>,
                          input_bytes:&[u8],
                          input_offset: &mut usize,
                          output_bytes:&mut [u8],
                          output_offset: &mut usize) -> BrotliResult {
        let literal_len = in_cmd.data.slice().len() as u32;
        let literal_nibble_len = (literal_len as usize) << 1;
        let lllen: u8 = (core::mem::size_of_val(&literal_len) as u32 * 8 - literal_len.leading_zeros()) as u8;
        let uniform_prob = CDF16::<FrequentistCDFUpdater>::default();
        loop {
            match superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset) {
                BrotliResult::ResultSuccess => {},
                need_something => return need_something,
            }
            match self.state {
                LiteralSubstate::Begin => {
                    let mut beg_nib = core::cmp::min(15, lllen);
                    superstate.coder.get_or_put_nibble(&mut beg_nib, &uniform_prob);
                    if beg_nib == 15 {
                        self.state = LiteralSubstate::LiteralCountLengthGreater14Less25;
                    } else if beg_nib <= 1 {
                        self.lc.data = AllocatedMemoryPrefix::<AllocU8>(superstate.m8.alloc_cell(beg_nib as usize),
                                                                        beg_nib as usize);
                        self.state = LiteralSubstate::LiteralNibbleIndex((beg_nib as u32) << 1);
                    } else {
                        self.state = LiteralSubstate::LiteralCountMantissaNibbles(round_up_mod_4(beg_nib - 1),
                                                                                  1 << (beg_nib));
                    }
                },
                LiteralSubstate::LiteralCountLengthGreater14Less25 => {
                    let mut last_nib = lllen - 15;
                    superstate.coder.get_or_put_nibble(&mut last_nib, &uniform_prob);
                    self.state = LiteralSubstate::LiteralCountMantissaNibbles(round_up_mod_4(last_nib + 14),
                                                                              1 << (last_nib + 15));
                },
                LiteralSubstate::LiteralCountMantissaNibbles(len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = (literal_len ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    superstate.coder.get_or_put_nibble(&mut last_nib, &uniform_prob);
                    let next_decoded_so_far = decoded_so_far | ((last_nib as u32) << next_len_remaining);
                
                    if next_len_remaining == 0 {
                        self.lc.data = AllocatedMemoryPrefix::<AllocU8>(superstate.m8.alloc_cell(next_decoded_so_far as usize),
                                                                      next_decoded_so_far as usize);
                        self.state = LiteralSubstate::LiteralNibbleIndex(next_decoded_so_far << 1);
                    } else {
                        self.state  = LiteralSubstate::LiteralCountMantissaNibbles(next_len_remaining,
                                                                                   next_decoded_so_far);
                    }
                },
                LiteralSubstate::LiteralNibbleIndex(index) => {
                    let mut cur_nibble = superstate.specialization.get_literal_nibble(in_cmd,
                                                                                  literal_nibble_len - 1 - index as usize);
                    superstate.coder.get_or_put_nibble(&mut cur_nibble, &uniform_prob);
                    self.lc.data.slice_mut()[index as usize >> 1] |= cur_nibble << ((index & 1) << 4);
                    if index != 0 {
                        self.state = LiteralSubstate::FullyDecoded;
                        return BrotliResult::ResultSuccess;
                    } else {
                        self.state = LiteralSubstate::LiteralNibbleIndex(index - 1);
                    }
                },
                LiteralSubstate::FullyDecoded => {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
}



enum EncodeOrDecodeState<AllocU8: Allocator<u8> > {
    Begin,
    Literal(LiteralState<AllocU8>),
    Dict(DictState),
    Copy(CopyState),
    PopulateRingBuffer,
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
    recoder: super::cmd_to_raw::DivansRecodeState<AllocU8::AllocatedMemory>, 
    m8: AllocU8,
}
pub struct DivansCodec<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                       Specialization:EncoderOrDecoderSpecialization,
                       AllocU8: Allocator<u8>> {
    cross_command_state: CrossCommandState<ArithmeticCoder,
                                           Specialization,
                                           AllocU8>,
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
    pub fn encode_or_decode<ISl:SliceWrapper<u8>+Default>(&mut self,
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
    pub fn encode_or_decode_one_command<ISl:SliceWrapper<u8>+Default>(&mut self,
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
                },
                &mut EncodeOrDecodeState::Copy(ref mut copy_state) => {
                    let backing_store = CopyCommand::nop();
                    let src_copy_command = self.cross_command_state.specialization.get_source_copy_command(input_cmd,
                                                                                                           &backing_store);
                    match copy_state.encode_or_decode(&mut self.cross_command_state,
                                                      src_copy_command,
                                                      input_bytes,
                                                      input_bytes_offset,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        BrotliResult::ResultSuccess => {
                            *o_cmd = Command::Copy(core::mem::replace(&mut copy_state.cc, CopyCommand::nop()));
                            new_state = Some(EncodeOrDecodeState::PopulateRingBuffer);
                        },
                        retval => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                },
                &mut EncodeOrDecodeState::Literal(ref mut lit_state) => {
                    let backing_store = LiteralCommand::nop();
                    let src_literal_command = self.cross_command_state.specialization.get_source_literal_command(input_cmd,
                                                                                                                 &backing_store);
                    match lit_state.encode_or_decode(&mut self.cross_command_state,
                                                      src_literal_command,
                                                      input_bytes,
                                                      input_bytes_offset,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        BrotliResult::ResultSuccess => {
                            *o_cmd = Command::Literal(core::mem::replace(&mut lit_state.lc,
                                                                         LiteralCommand::<AllocatedMemoryPrefix<AllocU8>>::nop()));
                            new_state = Some(EncodeOrDecodeState::PopulateRingBuffer);
                        },
                        retval => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                },
                &mut EncodeOrDecodeState::Dict(ref mut dict_state) => {
                    let backing_store = DictCommand::nop();
                    let src_dict_command = self.cross_command_state.specialization.get_source_dict_command(input_cmd,
                                                                                                                 &backing_store);
                    match dict_state.encode_or_decode(&mut self.cross_command_state,
                                                      src_dict_command,
                                                      input_bytes,
                                                      input_bytes_offset,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        BrotliResult::ResultSuccess => {
                            *o_cmd = Command::Dict(core::mem::replace(&mut dict_state.dc, DictCommand::nop()));
                            new_state = Some(EncodeOrDecodeState::PopulateRingBuffer);
                        },
                        retval => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                },
                &mut EncodeOrDecodeState::PopulateRingBuffer => {
                    let mut ioffset: usize = 0;
                    let mut tmp_o_cmd = [core::mem::replace(o_cmd, Command::nop())];
                    match self.cross_command_state.recoder.recode(&mut tmp_o_cmd,
                                                                  &mut ioffset,
                                                                  self.cross_command_state.
                                                                  specialization.get_recoder_output(output_bytes),
                                                                  self.cross_command_state.
                                                                  specialization.get_recoder_output_offset(output_bytes_offset)) {
                        BrotliResult::NeedsMoreInput => new_state = Some(EncodeOrDecodeState::Begin),
                        BrotliResult::NeedsMoreOutput => new_state = Some(EncodeOrDecodeState::PopulateRingBuffer),
                        BrotliResult::ResultFailure => {
                            *o_cmd = core::mem::replace(&mut tmp_o_cmd[0], Command::nop());
                            return OneCommandReturn::BufferExhausted(BrotliResult::ResultFailure);
                        },
                        BrotliResult::ResultSuccess => new_state = Some(EncodeOrDecodeState::Begin),
                    }
                    *o_cmd = core::mem::replace(&mut tmp_o_cmd[0], Command::nop())
                },
            }
            match new_state {
                Some(ns) => {
                    match ns {
                        EncodeOrDecodeState::Begin => {
                            self.state = EncodeOrDecodeState::Begin;
                            return OneCommandReturn::Advance;
                        },
                        _ => self.state = ns,
                    }
                },
                None => {},
            }
        }
    }
                        
}
