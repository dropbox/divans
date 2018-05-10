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

#![allow(dead_code)]
use core;
use core::hash::Hasher;
mod crc32;
mod crc32_table;
use self::crc32::{crc32c_init,crc32c_update};
use alloc::{SliceWrapper, Allocator};
use interface::{DivansResult, DivansOutputResult, ErrMsg, StreamMuxer, StreamDemuxer, StreamID, ReadableBytes};
use ::alloc_util::UninitializedOnAlloc;
pub const CMD_BUFFER_SIZE: usize = 16;
use ::alloc_util::RepurposingAlloc;
use super::interface::{
    BillingDesignation,
    CrossCommandBilling,
    BlockSwitch,
    LiteralBlockSwitch,
    NewWithAllocator,
    Nop
 
};
pub mod weights;
pub mod specializations;
use self::specializations::{
    construct_codec_trait_from_bookkeeping,
    CodecTraitSelector,
    CodecTraits,
};
mod interface;
use ::slice_util::AllocatedMemoryPrefix;
pub use self::interface::{
    StrideSelection,
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    CrossCommandBookKeeping,
    NUM_ARITHMETIC_CODERS,
    CMD_CODER,
};
use super::interface::{
    ArithmeticEncoderOrDecoder,
    Command,
    CopyCommand,
    DictCommand,
    LiteralCommand,
    PredictionModeContextMap,
};
pub mod copy;
pub mod dict;
pub mod literal;
pub mod context_map;
pub mod block_type;
pub mod priors;



/*
use std::io::Write;
macro_rules! println_stderr(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);
*/
use super::probability::{CDF2, CDF16, Speed};

//#[cfg(feature="billing")]
//use std::io::Write;
//#[cfg(feature="billing")]
//macro_rules! println_stderr(
//    ($($val:tt)*) => { {
//        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
//    } }
//);
//
//#[cfg(not(feature="billing"))]
//macro_rules! println_stderr(
//    ($($val:tt)*) => { {
////        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
//    } }
//);






#[derive(Clone,Copy,Debug)]
enum EncodeOrDecodeState {
    Begin,
    Literal,
    Dict,
    Copy,
    BlockSwitchLiteral,
    BlockSwitchCommand,
    BlockSwitchDistance,
    PredictionMode,
    PopulateRingBuffer,
    DivansSuccess,
    EncodedShutdownNode, // in flush/close state (encoder only) and finished flushing the EOF node type
    ShutdownCoder(StreamID),
    CoderBufferDrain,
    MuxDrain,
    WriteChecksum(u8),
}

const CHECKSUM_LENGTH: usize = 8;


impl Default for EncodeOrDecodeState {
    fn default() -> Self {
        EncodeOrDecodeState::Begin
    }
}



pub fn command_type_to_nibble<SliceType:SliceWrapper<u8>>(cmd:&Command<SliceType>,
                                                          is_end: bool) -> u8 {

    if is_end {
        return 0xf;
    }
    match *cmd {
        Command::Copy(_) => 0x1,
        Command::Dict(_) => 0x2,
        Command::Literal(_) => 0x3,
        Command::BlockSwitchLiteral(_) => 0x4,
        Command::BlockSwitchCommand(_) => 0x5,
        Command::BlockSwitchDistance(_) => 0x6,
        Command::PredictionMode(_) => 0x7,
    }
}

pub struct DivansCodec<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                       Specialization:EncoderOrDecoderSpecialization,
                       LinearInputBytes:StreamDemuxer<AllocU8>+Default,
                       LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                       Cdf16:CDF16,
                       AllocU8: Allocator<u8>,
                       AllocCDF2:Allocator<CDF2>,
                       AllocCDF16:Allocator<Cdf16>> {
    cross_command_state: CrossCommandState<ArithmeticCoder,
                                           Specialization,
                                           LinearInputBytes,
                                           LinearOutputBytes,
                                           Cdf16,
                                           AllocU8,
                                           AllocCDF2,
                                           AllocCDF16>,
    state: EncodeOrDecodeState,
    state_lit: literal::LiteralState<AllocU8>,
    state_copy: copy::CopyState,
    state_dict: dict::DictState,
    state_lit_block_switch: block_type::LiteralBlockTypeState,
    state_block_switch: block_type::BlockTypeState,
    state_prediction_mode: context_map::PredictionModeState<AllocU8>,
    state_populate_ring_buffer: Command<AllocatedMemoryPrefix<u8, AllocU8>>,
    codec_traits: CodecTraitSelector,
    crc: SubDigest,
    frozen_checksum: Option<u64>,
    skip_checksum: bool,
}

pub enum OneCommandReturn {
    Advance,
    BufferExhausted(DivansResult),
}
enum CodecTraitResult {
    Res(OneCommandReturn),
    UpdateCodecTraitAndAdvance(CodecTraitSelector),
}



impl<AllocU8: Allocator<u8>,
     ArithmeticCoder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>,
     Specialization: EncoderOrDecoderSpecialization,
     LinearInputBytes:StreamDemuxer<AllocU8>+Default,
     LinearOutputBytes:StreamMuxer<AllocU8>+Default,
     Cdf16:CDF16,
     AllocCDF2: Allocator<CDF2>,
     AllocCDF16:Allocator<Cdf16>> DivansCodec<ArithmeticCoder, Specialization, LinearInputBytes, LinearOutputBytes, Cdf16, AllocU8, AllocCDF2, AllocCDF16> {
    pub fn free(self) -> (AllocU8, AllocCDF2, AllocCDF16) {
        self.cross_command_state.free()
    }
    pub fn free_ref(&mut self) {
        self.state_prediction_mode.reset(&mut self.cross_command_state.m8);
        self.cross_command_state.m8.use_cached_allocation::<UninitializedOnAlloc>().free_cell(
            core::mem::replace(&mut self.state_lit.lc,
                               LiteralCommand::<AllocatedMemoryPrefix<u8, AllocU8>>::nop()).data);

        self.cross_command_state.free_ref()
    }
    pub fn new(m8:AllocU8,
               mcdf2:AllocCDF2,
               mcdf16:AllocCDF16,
               cmd_coder: ArithmeticCoder,
               lit_coder: ArithmeticCoder,
               specialization: Specialization,
               ring_buffer_size: usize,
               dynamic_context_mixing: u8,
               prior_depth: Option<u8>,
               literal_adaptation_rate: Option<[Speed;4]>,
               do_context_map: bool,
               force_stride: interface::StrideSelection,
               skip_checksum: bool) -> Self {
        let mut ret = DivansCodec::<ArithmeticCoder,  Specialization, LinearInputBytes, LinearOutputBytes, Cdf16, AllocU8, AllocCDF2, AllocCDF16> {
            cross_command_state:CrossCommandState::<ArithmeticCoder,
                                                    Specialization,
                                                    LinearInputBytes,
                                                    LinearOutputBytes,
                                                    Cdf16,
                                                    AllocU8,
                                                    AllocCDF2,
                                                    AllocCDF16>::new(m8,
                                                                     mcdf2,
                                                                     mcdf16,
                                                                     cmd_coder,
                                                                     lit_coder,
                                                                     specialization,
                                                                     ring_buffer_size,
                                                                     dynamic_context_mixing,
                                                                     prior_depth.unwrap_or(0),
                                                                     literal_adaptation_rate,
                                                                     do_context_map,
                                                                     force_stride,
            ),
            state:EncodeOrDecodeState::Begin,
            codec_traits: CodecTraitSelector::DefaultTrait(&specializations::DEFAULT_TRAIT),
            state_copy: copy::CopyState::begin(),
            state_dict: dict::DictState::begin(),
            state_lit: literal::LiteralState {
                lc:LiteralCommand::<AllocatedMemoryPrefix<u8, AllocU8>>::nop(),
                state:literal::LiteralSubstate::Begin,
            },
            state_lit_block_switch: block_type::LiteralBlockTypeState::begin(),
            state_block_switch: block_type::BlockTypeState::begin(),
            state_prediction_mode: context_map::PredictionModeState::nop(),
            state_populate_ring_buffer: Command::<AllocatedMemoryPrefix<u8, AllocU8>>::nop(),
            crc: default_crc(),
            frozen_checksum: None,
            skip_checksum:skip_checksum,
        };
        ret.codec_traits = construct_codec_trait_from_bookkeeping(&ret.cross_command_state.bk);
        ret
    }
    #[inline(always)]
    fn update_command_state_from_nibble(&mut self, command_type_code:u8, is_end: bool) -> DivansResult{
        match command_type_code {
            1 => {
                self.state_copy = copy::CopyState::begin();
                self.state = EncodeOrDecodeState::Copy;
            },
            2 => {
                self.state_dict = dict::DictState::begin();
                self.state = EncodeOrDecodeState::Dict;
            }
            
            3 => {
                self.state_lit = literal::LiteralState {
                    lc:LiteralCommand::<AllocatedMemoryPrefix<u8, AllocU8>>::nop(),
                    state:literal::LiteralSubstate::Begin,
                };
                self.state = EncodeOrDecodeState::Literal;
            },
            4 => {
                self.state_lit_block_switch = block_type::LiteralBlockTypeState::begin();
                self.state = EncodeOrDecodeState::BlockSwitchLiteral;
            },
            
            5 => {
                self.state_block_switch = block_type::BlockTypeState::begin();
                self.state = EncodeOrDecodeState::BlockSwitchCommand;
            },
            6 => {
                self.state_block_switch = block_type::BlockTypeState::begin();
                self.state = EncodeOrDecodeState::BlockSwitchDistance;
            },
            7 => {
                self.state_prediction_mode.reset(&mut self.cross_command_state.m8);
                self.state = EncodeOrDecodeState::PredictionMode;
            },
            0xf => if is_end {
                self.state = EncodeOrDecodeState::DivansSuccess; // encoder flows through this path
            } else {
                self.state = EncodeOrDecodeState::WriteChecksum(0);
            },
            _ => return DivansResult::Failure(ErrMsg::CommandCodeOutOfBounds(command_type_code)),
        };
        DivansResult::Success
    }
    #[inline(always)]
    pub fn get_coder(&self, index: StreamID) -> &ArithmeticCoder {
        &self.cross_command_state.coder[usize::from(index)]
    }
    #[inline(always)]
    pub fn get_m8(&mut self) -> &mut RepurposingAlloc<u8, AllocU8> {
        &mut self.cross_command_state.m8
    }
    #[inline(always)]
    pub fn specialization(&mut self) -> &mut Specialization{
        &mut self.cross_command_state.specialization
    }
    #[inline(always)]
    pub fn coder(&mut self, index: StreamID) -> &mut ArithmeticCoder {
        &mut self.cross_command_state.coder[usize::from(index)]
    }
    pub fn get_crc(&mut self) -> &mut SubDigest {
        &mut self.crc
    }
    pub fn flush(&mut self,
             output_bytes: &mut [u8],
             output_bytes_offset: &mut usize) -> DivansOutputResult{
        let adjusted_output_bytes = output_bytes.split_at_mut(*output_bytes_offset).1;
        let mut adjusted_output_bytes_offset = 0usize;
        let ret = self.internal_flush(adjusted_output_bytes, &mut adjusted_output_bytes_offset);
        *output_bytes_offset += adjusted_output_bytes_offset;
        match self.frozen_checksum {
            None => if !Specialization::IS_DECODING_FILE {
                self.crc.write(adjusted_output_bytes.split_at(adjusted_output_bytes_offset).0);
            },
            _ => {},
        }
        ret
    }
    fn internal_flush(&mut self,
                 output_bytes: &mut [u8],
                 output_bytes_offset: &mut usize) -> DivansOutputResult{
        let nop = Command::<AllocU8::AllocatedMemory>::nop();
        loop {
            match self.state {
                EncodeOrDecodeState::Begin => {
                    let mut unused = 0usize;
                    let mut unused = ReadableBytes{data:&[], read_offset: &mut unused};
                    match self.encode_or_decode_one_command(&mut unused,
                                                            output_bytes,
                                                            output_bytes_offset,
                                                            &nop,
                                                            &specializations::DEFAULT_TRAIT,
                                                            true) {
                        CodecTraitResult::Res(one_command_return) => match one_command_return {
                            OneCommandReturn::BufferExhausted(res) => {
                                match res {
                                    DivansResult::Success => {},
                                    DivansResult::NeedsMoreInput => return DivansOutputResult::Failure(ErrMsg::EncodeOneCommandNeedsInput),//"unreachable",//return DivansOutputResult::Success,
                                    DivansResult::NeedsMoreOutput => return DivansOutputResult::NeedsMoreOutput,
                                    DivansResult::Failure(m) => return DivansOutputResult::Failure(m),
                                }
                            },
                            OneCommandReturn::Advance => return DivansOutputResult::Failure(ErrMsg::UnintendedCodecState(3)),
                        },
                        CodecTraitResult::UpdateCodecTraitAndAdvance(_) => {
                            return DivansOutputResult::Failure(ErrMsg::UnintendedCodecState(4));
                        },
                    }
                    self.state = EncodeOrDecodeState::EncodedShutdownNode;
                },
                EncodeOrDecodeState::EncodedShutdownNode => {

                    for index in 0..NUM_ARITHMETIC_CODERS {
                        match self.cross_command_state.drain_or_fill_internal_buffer(index, output_bytes, output_bytes_offset) {
                            DivansResult::Success => if index + 1 == NUM_ARITHMETIC_CODERS {
                                self.state = EncodeOrDecodeState::ShutdownCoder(0);
                            },
                            DivansResult::NeedsMoreInput => return DivansOutputResult::Failure(ErrMsg::DrainOrFillNeedsInput(0)), // FIXME: is this possible?
                            DivansResult::NeedsMoreOutput => return DivansOutputResult::NeedsMoreOutput,
                            DivansResult::Failure(m) => return DivansOutputResult::Failure(m),
                        }
                    }
                },
                EncodeOrDecodeState::ShutdownCoder(index) => {
                    match self.cross_command_state.coder[usize::from(index)].close() {
                        DivansResult::Success => if index + 1 == NUM_ARITHMETIC_CODERS as u8 {
                            self.state = EncodeOrDecodeState::CoderBufferDrain;
                        } else {
                            self.state = EncodeOrDecodeState::ShutdownCoder(index + 1);
                        },
                        DivansResult::NeedsMoreInput => return DivansOutputResult::Failure(ErrMsg::ShutdownCoderNeedsInput), // FIXME: is this possible?
                        DivansResult::NeedsMoreOutput => return DivansOutputResult::NeedsMoreOutput,
                        DivansResult::Failure(m) => return DivansOutputResult::Failure(m),
                    }
                },
                EncodeOrDecodeState::CoderBufferDrain => {
                    for index in 0..NUM_ARITHMETIC_CODERS {
                        match self.cross_command_state.drain_or_fill_internal_buffer(index,
                                                                                     output_bytes,
                                                                                     output_bytes_offset) {
                            DivansResult::Success => if index + 1 == NUM_ARITHMETIC_CODERS {
                                self.state = EncodeOrDecodeState::MuxDrain;
                            },
                            DivansResult::NeedsMoreInput => return DivansOutputResult::Failure(ErrMsg::DrainOrFillNeedsInput(1)), // FIXME: is this possible?
                            DivansResult::NeedsMoreOutput => return DivansOutputResult::NeedsMoreOutput,
                            DivansResult::Failure(m) => return DivansOutputResult::Failure(m),
                        }
                    }
                }
                EncodeOrDecodeState::MuxDrain => {
                    loop {
                        let output_loc = output_bytes.split_at_mut(*output_bytes_offset).1;
                        if output_loc.len() == 0 {
                            return DivansOutputResult::NeedsMoreOutput;
                        }
                        let amt = self.cross_command_state.muxer.flush(output_loc);
                        *output_bytes_offset += amt;
                        if self.cross_command_state.muxer.wrote_eof() {
                            break;
                        }
                    }
                    self.state = EncodeOrDecodeState::WriteChecksum(0);
                },
                EncodeOrDecodeState::WriteChecksum(count) => {
                    match self.frozen_checksum {
                        None => {
                            if !Specialization::IS_DECODING_FILE {
                                self.crc.write(output_bytes.split_at(*output_bytes_offset).0);
                            }
                            self.frozen_checksum = Some(self.crc.finish());
                        },
                        _ => {},
                    };
                    let crc = self.frozen_checksum.unwrap();
                    let bytes_remaining = output_bytes.len() - *output_bytes_offset;
                    let checksum_cur_index = count as usize;
                    let bytes_needed = CHECKSUM_LENGTH - count as usize;

                    let count_to_copy = core::cmp::min(bytes_remaining,
                                                       bytes_needed);
                    assert!(crc <= 0xffffffff);
                    let checksum = [crc as u8 & 255,
                                    (crc >> 8) as u8 & 255,
                                    (crc >> 16) as u8 & 255,
                                    (crc >> 24) as u8 & 255,
                                    b'a',
                                    b'n',
                                    b's',
                                    b'~'];
                    output_bytes.split_at_mut(*output_bytes_offset).1.split_at_mut(
                        count_to_copy).0.clone_from_slice(checksum.split_at(checksum_cur_index).1.split_at(count_to_copy).0);
                    *output_bytes_offset += count_to_copy;
                    if bytes_needed <= bytes_remaining {
                        self.state = EncodeOrDecodeState::DivansSuccess;
                        return DivansOutputResult::Success;
                    } else {
                        self.state = EncodeOrDecodeState::WriteChecksum(count + count_to_copy as u8);
                        return DivansOutputResult::NeedsMoreOutput;
                    }
                },
                EncodeOrDecodeState::DivansSuccess => return DivansOutputResult::Success,
                // not allowed to flush if previous command was partially processed
                _ => return DivansOutputResult::Failure(ErrMsg::NotAllowedToFlushIfPreviousCommandPartial),
            }
        }
    }
    pub fn encode_or_decode<ISl:SliceWrapper<u8>+Default>(&mut self,
                                                          input_bytes: &[u8],
                                                          input_bytes_offset: &mut usize,
                                                          output_bytes: &mut [u8],
                                                          output_bytes_offset: &mut usize,
                                                          input_commands: &[Command<ISl>],
                                                          input_command_offset: &mut usize) -> DivansResult {
        let adjusted_output_bytes = output_bytes.split_at_mut(*output_bytes_offset).1;
        let mut adjusted_output_bytes_offset = 0usize;
        {
            let adjusted_input_bytes = input_bytes.split_at(*input_bytes_offset).1;
            let adjusted_input_bytes_offset = self.cross_command_state.demuxer.write_linear(
                adjusted_input_bytes,
                self.cross_command_state.m8.get_base_alloc());
            if Specialization::IS_DECODING_FILE && !self.skip_checksum {
                self.crc.write(adjusted_input_bytes.split_at(adjusted_input_bytes_offset).0);
            }
            *input_bytes_offset += adjusted_input_bytes_offset;
        }
        let mut checksum_input_info = ReadableBytes{data:input_bytes, read_offset:input_bytes_offset};
        loop {
            let res:(Option<DivansResult>, Option<CodecTraitSelector>);
            match self.codec_traits {
                CodecTraitSelector::MixingTrait(tr) => res = self.e_or_d_specialize(&mut checksum_input_info,
                                                                                    adjusted_output_bytes,
                                                                                    &mut adjusted_output_bytes_offset,
                                                                                    input_commands,
                                                                                    input_command_offset,
                                                                                    tr),
                CodecTraitSelector::DefaultTrait(tr) => res = self.e_or_d_specialize(&mut checksum_input_info,
                                                                                     adjusted_output_bytes,
                                                                                     &mut adjusted_output_bytes_offset,
                                                                                     input_commands,
                                                                                     input_command_offset,
                                                                                     tr),
            }
            if let Some(update) = res.1 {
                self.codec_traits = update;
            }
            if let Some(result) = res.0 {
                *output_bytes_offset += adjusted_output_bytes_offset;
                match self.frozen_checksum {
                    Some(_) => {},
                    None => if !Specialization::IS_DECODING_FILE {
                        self.crc.write(&adjusted_output_bytes.split_at(adjusted_output_bytes_offset).0);
                    },
                }
                return result;
            }
        }
    }
    fn e_or_d_specialize<ISl:SliceWrapper<u8>+Default,
                         CTraits:CodecTraits>(&mut self,
                                              checksum_input_info: &mut ReadableBytes,
                                              output_bytes: &mut [u8],
                                              output_bytes_offset: &mut usize,
                                              input_commands: &[Command<ISl>],
                                              input_command_offset: &mut usize,
                                              ctraits: &'static CTraits) -> (Option<DivansResult>, Option<CodecTraitSelector>) {
        let i_cmd_backing = Command::<ISl>::nop();
        loop {
            let in_cmd = self.cross_command_state.specialization.get_input_command(input_commands,
                                                                                   *input_command_offset,
                                                                                   &i_cmd_backing);
            match self.encode_or_decode_one_command(checksum_input_info,
                                                    output_bytes,
                                                    output_bytes_offset,
                                                    in_cmd,
                                                    ctraits,
                                                    false /* not end*/) {
                CodecTraitResult::Res(one_command_return) => match one_command_return {
                    OneCommandReturn::Advance => {
                        *input_command_offset += 1;
                        if input_commands.len() == *input_command_offset {
                            return (Some(DivansResult::NeedsMoreInput), None);
                        }
                    },
                    OneCommandReturn::BufferExhausted(result) => {
                        return (Some(result), None);
                    }
                },
                CodecTraitResult::UpdateCodecTraitAndAdvance(cts) => {
                    *input_command_offset += 1;
                    if input_commands.len() == *input_command_offset {
                        return (Some(DivansResult::NeedsMoreInput), Some(cts));
                    }
                    return (None, Some(cts));
                },
            }
        }
    }
    fn encode_or_decode_one_command<ISl:SliceWrapper<u8>+Default,
                                    CTraits:CodecTraits>(&mut self,
                                                         checksum_input_info: &mut ReadableBytes,
                                                         output_bytes: &mut [u8],
                                                         output_bytes_offset: &mut usize,
                                                         input_cmd: &Command<ISl>,
                                                         ctraits: &'static CTraits,
                                                         is_end: bool) -> CodecTraitResult {
        loop {
            match self.state {
                EncodeOrDecodeState::EncodedShutdownNode
                    | EncodeOrDecodeState::ShutdownCoder(_)
                    | EncodeOrDecodeState::CoderBufferDrain
                        | EncodeOrDecodeState::MuxDrain => {
                    // not allowed to encode additional commands after flush is invoked
                    return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::Failure(ErrMsg::NotAllowedToEncodeAfterFlush)));
                },
                EncodeOrDecodeState::WriteChecksum(count) => {
                    assert!(Specialization::IS_DECODING_FILE);
                    if !self.cross_command_state.demuxer.encountered_eof() {
                        return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::NeedsMoreInput));
                    }
                    if self.skip_checksum {
                        self.frozen_checksum = Some(0);
                    }
                    // decoder only operation
                    let checksum_cur_index = count;
                    let bytes_needed = CHECKSUM_LENGTH - count as usize;

                    let to_check = core::cmp::min(checksum_input_info.data.len() - *checksum_input_info.read_offset,
                                                  bytes_needed);
                    if to_check == 0 {
                        return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::NeedsMoreInput));
                    }
                    match self.frozen_checksum {
                        Some(_) => {},
                        None => {
                            //DO NOT DO AGAIN; self.crc.write(checksum_input_info.data.split_at(*checksum_input_info.read_offset).0); ALREADY DONE
                            self.frozen_checksum= Some(self.crc.finish());
                        },
                    }
                    let crc = self.frozen_checksum.unwrap();
                    assert!(crc <= 0xffffffff);
                    let checksum = [crc as u8 & 255,
                                    (crc >> 8) as u8 & 255,
                                    (crc >> 16) as u8 & 255,
                                    (crc >> 24) as u8 & 255,
                                    b'a',
                                    b'n',
                                    b's',
                                    b'~'];

                    for (index, (chk, fil)) in checksum.split_at(checksum_cur_index as usize).1.split_at(to_check).0.iter().zip(
                        checksum_input_info.data.split_at(*checksum_input_info.read_offset).1.split_at(to_check).0.iter()).enumerate() {
                        if *chk != *fil {
                            if checksum_cur_index as usize + index >= 4 || !self.skip_checksum {
                                return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::Failure(
                                    ErrMsg::BadChecksum(*chk, *fil))));
                            }
                        }
                    }
                    *checksum_input_info.read_offset += to_check;
                    if bytes_needed != to_check {
                        self.state = EncodeOrDecodeState::WriteChecksum(count as u8 + to_check as u8);
                    } else {
                        self.state = EncodeOrDecodeState::DivansSuccess;
                    }
                },
                EncodeOrDecodeState::DivansSuccess => {
                    return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::Success));
                },
                EncodeOrDecodeState::Begin => {
                    match self.cross_command_state.drain_or_fill_internal_buffer(CMD_CODER, output_bytes, output_bytes_offset) {
                        DivansResult::Success => {},
                        need_something => return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(need_something)),
                    }
                    let mut command_type_code = command_type_to_nibble(input_cmd, is_end);
                    {
                        let command_type_prob = self.cross_command_state.bk.get_command_type_prob();
                        self.cross_command_state.coder[CMD_CODER].get_or_put_nibble(
                            &mut command_type_code,
                            command_type_prob,
                            BillingDesignation::CrossCommand(CrossCommandBilling::FullSelection));
                        command_type_prob.blend(command_type_code, Speed::ROCKET);
                    }
                    match self.update_command_state_from_nibble(command_type_code, is_end) {
                        DivansResult::Success => {},
                        need_something => return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(need_something)),
                    }
                    match self.state {
                        EncodeOrDecodeState::Copy => { self.cross_command_state.bk.obs_copy_state(); },
                        EncodeOrDecodeState::Dict => { self.cross_command_state.bk.obs_dict_state(); },
                        EncodeOrDecodeState::Literal => { self.cross_command_state.bk.obs_literal_state(); },
                        _ => {},
                    }
                },
                EncodeOrDecodeState::PredictionMode => {
                    let default_prediction_mode_context_map = PredictionModeContextMap::<ISl> {
                        literal_context_map:ISl::default(),
                        predmode_speed_and_distance_context_map:ISl::default(),
                    };
                    let src_pred_mode = match *input_cmd {
                        Command::PredictionMode(ref pm) => pm,
                        _ => &default_prediction_mode_context_map,
                     };
                     match self.state_prediction_mode.encode_or_decode(&mut self.cross_command_state,
                                                                  src_pred_mode,
                                                                  output_bytes,
                                                                  output_bytes_offset) {
                         DivansResult::Success => {
                             {
                                 let _ret = &mut self.state_prediction_mode.pm;
                             }
                             self.state_prediction_mode.reset(&mut self.cross_command_state.m8);
                             self.state = EncodeOrDecodeState::Begin;
                             return CodecTraitResult::UpdateCodecTraitAndAdvance(
                                 construct_codec_trait_from_bookkeeping(&self.cross_command_state.bk));
                         },
                         // this odd new_state command will tell the downstream to readjust the predictors
                         retval => return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(retval)),
                    }
                },
                EncodeOrDecodeState::BlockSwitchLiteral => {
                    let src_block_switch_literal = match *input_cmd {
                        Command::BlockSwitchLiteral(bs) => bs,
                        _ => LiteralBlockSwitch::default(),
                    };
                    match self.state_lit_block_switch.encode_or_decode(&mut self.cross_command_state,
                                                            src_block_switch_literal,
                                                            output_bytes,
                                                            output_bytes_offset) {
                        DivansResult::Success => {
                            let old_stride = self.cross_command_state.bk.stride;
                            self.cross_command_state.bk.obs_btypel(match self.state_lit_block_switch {
                                block_type::LiteralBlockTypeState::FullyDecoded(btype, stride) => LiteralBlockSwitch::new(btype, stride),
                                _ => return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(
                                    DivansResult::Failure(ErrMsg::UnintendedCodecState(0)))),
                            });
                            if (old_stride <= 1) != (self.cross_command_state.bk.stride <= 1) {
                                self.state = EncodeOrDecodeState::Begin;
                                return CodecTraitResult::UpdateCodecTraitAndAdvance(
                                    construct_codec_trait_from_bookkeeping(&self.cross_command_state.bk));
                                // we need to chage to update codec trait
                            } else {
                                self.state = EncodeOrDecodeState::Begin;
                                return CodecTraitResult::Res(OneCommandReturn::Advance);
                            }
                        },
                        retval => {
                            return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(retval));
                        }
                    }
                },
                EncodeOrDecodeState::BlockSwitchCommand => {
                    let src_block_switch_command = match *input_cmd {
                        Command::BlockSwitchCommand(bs) => bs,
                        _ => BlockSwitch::default(),
                    };
                    match self.state_block_switch.encode_or_decode(&mut self.cross_command_state,
                                                            src_block_switch_command,
                                                            self::interface::BLOCK_TYPE_COMMAND_SWITCH,
                                                            output_bytes,
                                                            output_bytes_offset) {
                        DivansResult::Success => {
                            self.cross_command_state.bk.obs_btypec(match self.state_block_switch {
                                block_type::BlockTypeState::FullyDecoded(btype) => btype,
                                _ => return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(
                                    DivansResult::Failure(ErrMsg::UnintendedCodecState(1)))),
                            });
                            self.state = EncodeOrDecodeState::Begin;
                            return CodecTraitResult::Res(OneCommandReturn::Advance);
                        },
                        retval => {
                            return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(retval));
                        }
                    }
                },
                EncodeOrDecodeState::BlockSwitchDistance => {
                    let src_block_switch_distance = match *input_cmd {
                        Command::BlockSwitchDistance(bs) => bs,
                        _ => BlockSwitch::default(),
                    };

                    match self.state_block_switch.encode_or_decode(&mut self.cross_command_state,
                                                            src_block_switch_distance,
                                                            self::interface::BLOCK_TYPE_DISTANCE_SWITCH,
                                                            output_bytes,
                                                            output_bytes_offset) {
                        DivansResult::Success => {
                            self.cross_command_state.bk.obs_btyped(match self.state_block_switch {
                                block_type::BlockTypeState::FullyDecoded(btype) => btype,
                                _ => return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(
                                    DivansResult::Failure(ErrMsg::UnintendedCodecState(2)))),
                            });
                            self.state = EncodeOrDecodeState::Begin;
                            return CodecTraitResult::Res(OneCommandReturn::Advance);
                        },
                        retval => {
                            return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(retval));
                        }
                    }
                },
                EncodeOrDecodeState::Copy => {
                    let backing_store = CopyCommand{
                        distance:1,
                        num_bytes:0,
                    };
                    let src_copy_command = self.cross_command_state.specialization.get_source_copy_command(input_cmd,
                                                                                                           &backing_store);
                    match self.state_copy.encode_or_decode(&mut self.cross_command_state,
                                                      src_copy_command,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        DivansResult::Success => {
                            self.cross_command_state.bk.obs_distance(&self.state_copy.cc);
                            self.state_populate_ring_buffer = Command::Copy(core::mem::replace(
                                &mut self.state_copy.cc,
                                CopyCommand{distance:1, num_bytes:0}));
                            self.state = EncodeOrDecodeState::PopulateRingBuffer;
                        },
                        retval => {
                            return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(retval));
                        }
                    }
                },
                EncodeOrDecodeState::Literal => {
                    let backing_store = LiteralCommand::nop();
                    let src_literal_command = self.cross_command_state.specialization.get_source_literal_command(
                        input_cmd,
                        &backing_store);
                    match self.state_lit.encode_or_decode(&mut self.cross_command_state,
                                                     src_literal_command,
                                                     output_bytes,
                                                     output_bytes_offset,
                                                     ctraits) {
                        DivansResult::Success => {
                            self.state_populate_ring_buffer = Command::Literal(
                                core::mem::replace(&mut self.state_lit.lc,
                                                   LiteralCommand::<AllocatedMemoryPrefix<u8, AllocU8>>::nop()));
                            self.state = EncodeOrDecodeState::PopulateRingBuffer;
                        },
                        retval => {
                            return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(retval));
                        }
                    }
                },
                EncodeOrDecodeState::Dict => {
                    let backing_store = DictCommand::nop();
                    let src_dict_command = self.cross_command_state.specialization.get_source_dict_command(input_cmd,
                                                                                                                 &backing_store);
                    match self.state_dict.encode_or_decode(&mut self.cross_command_state,
                                                      src_dict_command,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        DivansResult::Success => {
                            self.state_populate_ring_buffer = Command::Dict(
                                core::mem::replace(&mut self.state_dict.dc,
                                                   DictCommand::nop()));
                            self.state = EncodeOrDecodeState::PopulateRingBuffer;
                        },
                        retval => {
                            return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(retval));
                        }
                    }
                },
                EncodeOrDecodeState::PopulateRingBuffer => {
                    let mut tmp_output_offset_bytes_backing: usize = 0;
                    let mut tmp_output_offset_bytes = self.cross_command_state.specialization.get_recoder_output_offset(
                        output_bytes_offset,
                        &mut tmp_output_offset_bytes_backing);
                    match self.cross_command_state.recoder.encode_cmd(&mut self.state_populate_ring_buffer,
                                                                  self.cross_command_state.
                                                                  specialization.get_recoder_output(output_bytes),
                                                                  tmp_output_offset_bytes) {
                        DivansOutputResult::NeedsMoreOutput => {
                            self.cross_command_state.bk.decode_byte_count = self.cross_command_state.recoder.num_bytes_encoded() as u32;
                            if Specialization::DOES_CALLER_WANT_ORIGINAL_FILE_BYTES {
                                return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::NeedsMoreOutput)); // we need the caller to drain the buffer
                            }
                        },
                        DivansOutputResult::Failure(m) => {
                            self.cross_command_state.bk.decode_byte_count = self.cross_command_state.recoder.num_bytes_encoded() as u32;
                            return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::Failure(m)));
                        },
                        DivansOutputResult::Success => {
                            self.cross_command_state.bk.command_count += 1;
                            self.cross_command_state.bk.decode_byte_count = self.cross_command_state.recoder.num_bytes_encoded() as u32;
                            // clobber bk.last_8_literals with the last 8 literals
                            let last_8 = self.cross_command_state.recoder.last_8_literals();
                            self.cross_command_state.bk.last_8_literals =
                                u64::from(last_8[0])
                                | (u64::from(last_8[1])<<0x8)
                                | (u64::from(last_8[2])<<0x10)
                                | (u64::from(last_8[3])<<0x18)
                                | (u64::from(last_8[4])<<0x20)
                                | (u64::from(last_8[5])<<0x28)
                                | (u64::from(last_8[6])<<0x30)
                                | (u64::from(last_8[7])<<0x38);
                            self.state = EncodeOrDecodeState::Begin;
                            match &mut self.state_populate_ring_buffer {
                                &mut Command::Literal(ref mut l) => {
                                    let mfd = core::mem::replace(
                                        &mut l.data,
                                        AllocatedMemoryPrefix::<u8, AllocU8>::default());
                                    self.cross_command_state.m8.use_cached_allocation::<
                                            UninitializedOnAlloc>().free_cell(mfd);
                                    //FIXME: what about prob array: should that be freed
                                },
                                &mut Command::Dict(_) |
                                &mut Command::Copy(_) |
                                &mut Command::BlockSwitchCommand(_) |
                                &mut Command::BlockSwitchLiteral(_) |
                                &mut Command::BlockSwitchDistance(_) => {
                                },
                                &mut Command::PredictionMode(ref mut pm) => {
                                    let mfd = core::mem::replace(
                                        &mut pm.literal_context_map,
                                        AllocatedMemoryPrefix::<u8, AllocU8>::default());
                                    self.cross_command_state.m8.use_cached_allocation::<
                                            UninitializedOnAlloc>().free_cell(mfd);
                                    let mfd = core::mem::replace(
                                        &mut pm.predmode_speed_and_distance_context_map,
                                        AllocatedMemoryPrefix::<u8, AllocU8>::default());
                                    self.cross_command_state.m8.use_cached_allocation::<
                                            UninitializedOnAlloc>().free_cell(mfd);
                                },
                            }
                            return CodecTraitResult::Res(OneCommandReturn::Advance);
                        },
                    }
                },
            }
        }
    }
}

pub struct SubDigest(u32);
impl core::hash::Hasher for SubDigest {
    #[inline(always)]
    fn write(&mut self, data:&[u8]) {
        self.0 = crc32c_update(self.0, data)
    }
    #[inline(always)]
    fn finish(&self) -> u64 {
        u64::from(self.0)
    }
}
pub fn default_crc() -> SubDigest {
    SubDigest(crc32c_init())
}
/*
pub struct SubDigest(crc::crc64::Digest);
impl core::hash::Hasher for SubDigest {
    #[inline(always)]
    fn write(&mut self, data:&[u8]) {
        self.0.write(data)
            
    }
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.0.finish()
    }
}
pub fn default_crc() -> SubDigest {
    SubDigest(crc::crc64::Digest::new(crc::crc64::ECMA))
}

*/
