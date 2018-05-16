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
use alloc::{SliceWrapper, Allocator};
use interface::{DivansResult, DivansOutputResult, DivansOpResult, ErrMsg, StreamMuxer, StreamDemuxer, StreamID, ReadableBytes};
use ::alloc_util::UninitializedOnAlloc;
use mux::Mux;
pub const CMD_BUFFER_SIZE: usize = 16;
use ::alloc_util::RepurposingAlloc;
use super::interface::{
    BillingDesignation,
    CrossCommandBilling,
    BlockSwitch,
    LiteralBlockSwitch,
    NewWithAllocator,
    Nop,
};
pub mod weights;
pub mod specializations;
pub mod crc32;
pub mod crc32_table;
use self::specializations::{
    construct_codec_trait_from_bookkeeping,
    CodecTraitSelector,
    CodecTraits,
};
mod interface;
use threading::{CommandResult, ThreadToMain, MainToThread};
use ::slice_util::AllocatedMemoryPrefix;
pub use self::interface::{
    ThreadContext,
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
    free_cmd,
};
pub mod io;
pub mod copy;
pub mod dict;
pub mod literal;
pub mod context_map;
pub mod block_type;
pub mod priors;
pub mod decoder;
pub use self::decoder::{
    DivansDecoderCodec,
    SubDigest,
    default_crc,
};


/*
use std::io::Write;
macro_rules! println_stderr(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);
*/
use super::probability::{CDF16, Speed};

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
                       LinearInputBytes:StreamDemuxer<AllocU8>+ThreadToMain<AllocU8>+Default,
                       LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                       Cdf16:CDF16,
                       AllocU8: Allocator<u8>,
                       AllocCDF16:Allocator<Cdf16>> {
    cross_command_state: CrossCommandState<ArithmeticCoder,
                                           Specialization,
                                           LinearInputBytes,
                                           LinearOutputBytes,
                                           Cdf16,
                                           AllocU8,
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
     LinearInputBytes:StreamDemuxer<AllocU8>+ThreadToMain<AllocU8>+Default,
     LinearOutputBytes:StreamMuxer<AllocU8>+Default,
     Cdf16:CDF16,
     AllocCDF16:Allocator<Cdf16>> DivansCodec<ArithmeticCoder, Specialization, LinearInputBytes, LinearOutputBytes, Cdf16, AllocU8, AllocCDF16> {
    pub fn new(m8:AllocU8,
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
        let mut cross_command_state = CrossCommandState::<ArithmeticCoder,
                                                    Specialization,
                                                    LinearInputBytes,
                                                    LinearOutputBytes,
                                                    Cdf16,
                                                    AllocU8,
                                                    AllocCDF16>::new(m8,
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
        );

        let pm = context_map::PredictionModeState::begin(cross_command_state.thread_ctx.m8().unwrap());

        let mut ret = DivansCodec::<ArithmeticCoder,  Specialization, LinearInputBytes, LinearOutputBytes, Cdf16, AllocU8, AllocCDF16> {
            cross_command_state:cross_command_state,
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
            state_prediction_mode: pm,
            state_populate_ring_buffer: Command::<AllocatedMemoryPrefix<u8, AllocU8>>::nop(),
            crc: default_crc(),
            frozen_checksum: None,
            skip_checksum:skip_checksum,
        };
        match ret.cross_command_state.thread_ctx.lbk() {
            Some(ref book_keeping) => ret.codec_traits = construct_codec_trait_from_bookkeeping(book_keeping),
            None => {}, // FIXME(threading) don't need traits if we aren't processing literals
        }
        ret
    }
    pub fn join(&mut self,
                mut decoder: DivansDecoderCodec<Cdf16,
                                                AllocU8,
                                                AllocCDF16,
                                                ArithmeticCoder,
                                                Mux<AllocU8>>) {
        if let Some(ref mut ring_buffer_state) = decoder.state_populate_ring_buffer {
            free_cmd(ring_buffer_state, &mut decoder.ctx.m8.use_cached_allocation::<UninitializedOnAlloc>());
        }
        self.crc = decoder.crc;
        decoder.ctx.m8.use_cached_allocation::<
                UninitializedOnAlloc>().free_cell(core::mem::replace(&mut self.state_lit.lc.data, AllocatedMemoryPrefix::<u8, AllocU8>::default()));
        self.skip_checksum = decoder.skip_checksum;
        self.frozen_checksum = decoder.frozen_checksum;
        decoder.demuxer.free(&mut decoder.ctx.m8.get_base_alloc());
        let old_thread_context = core::mem::replace(&mut self.cross_command_state.thread_ctx, ThreadContext::MainThread(decoder.ctx));
        match old_thread_context {
            ThreadContext::MainThread(_) => panic!("Tried to join the main thread"),
            ThreadContext::Worker => {},
        };
    }
    pub fn fork(&mut self) -> DivansDecoderCodec<Cdf16,
                                                 AllocU8,
                                                 AllocCDF16,
                                                 ArithmeticCoder,
                                                 Mux<AllocU8>> {
        let skip_checksum = self.skip_checksum;
        if let Some(_) = self.frozen_checksum {
            panic!("Tried to fork() when checksum was already computed");
        }
        self.skip_checksum = true;
        let old_thread_context = core::mem::replace(&mut self.cross_command_state.thread_ctx, ThreadContext::Worker);
        let main_thread_context = match old_thread_context {
            ThreadContext::MainThread(mt) => mt,
            ThreadContext::Worker => panic!("Tried to fork from a Worker"),
        };
        DivansDecoderCodec::<Cdf16,
                             AllocU8,
                             AllocCDF16,
                             ArithmeticCoder,
                             Mux<AllocU8>>::new(main_thread_context, self.crc.clone(), skip_checksum)
    }
    pub fn demuxer(&mut self) -> &mut LinearInputBytes{
        &mut self.cross_command_state.demuxer
    }
    pub fn free(self) -> (AllocU8, AllocCDF16) {
        self.cross_command_state.free()
    }
    pub fn free_ref(&mut self) {
        self.state_prediction_mode.reset(self.cross_command_state.thread_ctx.m8().unwrap());
        self.cross_command_state.thread_ctx.m8().unwrap().use_cached_allocation::<UninitializedOnAlloc>().free_cell(
            core::mem::replace(&mut self.state_lit.lc,
                               LiteralCommand::<AllocatedMemoryPrefix<u8, AllocU8>>::nop()).data);

        self.cross_command_state.free_ref()
    }
    #[inline(always)]
    fn update_command_state_from_nibble(&mut self, command_type_code:u8, is_end: bool) -> DivansResult{
        self.cross_command_state.bk.command_count += 1;
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
                self.state_prediction_mode.state = context_map::PredictionModeSubstate::Begin;                
                self.state = EncodeOrDecodeState::PredictionMode;
            },
            0xf => if is_end {
                self.state = EncodeOrDecodeState::DivansSuccess; // encoder flows through this path
            } else {
                self.state = EncodeOrDecodeState::WriteChecksum(0)
            },
            _ => return DivansResult::Failure(ErrMsg::CommandCodeOutOfBounds(command_type_code)),
        };
        DivansResult::Success
    }
    #[inline(always)]
    pub fn get_coder(&self, index: StreamID) -> &ArithmeticCoder {
        if index == CMD_CODER as StreamID {
            &self.cross_command_state.coder
        } else {
            if let ThreadContext::MainThread(ref ctx) = self.cross_command_state.thread_ctx {
                &ctx.lit_coder
            } else {
                unreachable!();
            }
        }
    }
    #[inline(always)]
    pub fn coder_mut(&mut self, index: StreamID) -> &mut ArithmeticCoder {
        if index == CMD_CODER as StreamID {
            &mut self.cross_command_state.coder
        } else {
            &mut self.cross_command_state.thread_ctx.main_thread_mut().unwrap().lit_coder
        }
    }
    #[inline(always)]
    pub fn get_m8(&mut self) -> Option<&mut RepurposingAlloc<u8, AllocU8>> {
        self.cross_command_state.thread_ctx.m8() // FIXME(threading) usage of this shall be limited
    }
    #[inline(always)]
    pub fn specialization(&mut self) -> &mut Specialization{
        &mut self.cross_command_state.specialization
    }
    #[inline(always)]
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
                        let ret = if index == CMD_CODER {
                            self.cross_command_state.drain_or_fill_internal_buffer_cmd(output_bytes, output_bytes_offset)
                        } else {
                            self.cross_command_state.drain_or_fill_internal_buffer_lit(output_bytes, output_bytes_offset)
                        };
                        match ret {
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
                    match self.coder_mut(index as StreamID).close() {
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
                        let ret = if index == CMD_CODER {
                            self.cross_command_state.drain_or_fill_internal_buffer_cmd(output_bytes, output_bytes_offset)
                        } else {
                            self.cross_command_state.drain_or_fill_internal_buffer_lit(output_bytes, output_bytes_offset)
                        };
                        
                        match ret {
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
        if let Some(ref mut m8) = self.cross_command_state.thread_ctx.m8() {
            let adjusted_input_bytes = input_bytes.split_at(*input_bytes_offset).1;
            let adjusted_input_bytes_offset = self.cross_command_state.demuxer.write_linear(
                adjusted_input_bytes,
                m8.get_base_alloc());
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
                    match self.cross_command_state.thread_ctx {
                        // only main thread can checksum
                        ThreadContext::MainThread(_) => {},
                        ThreadContext::Worker => {
                            let (ret, _cmd, _mem) = self.cross_command_state.demuxer.push_command(
                                CommandResult::Eof,
                                None, None,
                                &mut self.cross_command_state.specialization,
                                output_bytes, output_bytes_offset);
                            match ret {
                                DivansOutputResult::Success => {
                                    self.state = EncodeOrDecodeState::DivansSuccess;
                                    continue;
                                },
                                r => return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::from(r))),
                            }
                        },
                    }
                    if !self.cross_command_state.demuxer.consumed_all_streams_until_eof() {
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
                    match self.cross_command_state.drain_or_fill_internal_buffer_cmd(output_bytes, output_bytes_offset) {
                        DivansResult::Success => {},
                        need_something => return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(need_something)),
                    }
                    let mut command_type_code = command_type_to_nibble(input_cmd, is_end);
                    {
                        let command_type_prob = self.cross_command_state.bk.get_command_type_prob();
                        self.cross_command_state.coder.get_or_put_nibble(
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
                    if !self.state_prediction_mode.pm.has_context_speeds() {
                        self.state_prediction_mode.pm =
                            match if let ThreadContext::MainThread(ref mut ctx) = self.cross_command_state.thread_ctx {
                                self.cross_command_state.demuxer.pull_context_map(Some(&mut ctx.m8))
                            } else {
                                self.cross_command_state.demuxer.pull_context_map(None)
                            } {
                                Ok(pm) => pm,
                                Err(_) => return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::NeedsMoreOutput)),
                            };
                    }

                    let default_prediction_mode_context_map = empty_prediction_mode_context_map::<ISl>();
                    let src_pred_mode = match *input_cmd {
                        Command::PredictionMode(ref pm) => pm,
                        _ => &default_prediction_mode_context_map,
                     };
                     match self.state_prediction_mode.encode_or_decode(&mut self.cross_command_state,
                                                                  src_pred_mode,
                                                                  output_bytes,
                                                                  output_bytes_offset) {
                         DivansResult::Success => {
                             if let ThreadContext::MainThread(ref mut ctx) = self.cross_command_state.thread_ctx {
                                 self.state = EncodeOrDecodeState::Begin;
                                 let ret = ctx.lbk.obs_prediction_mode_context_map(
                                     &self.state_prediction_mode.pm,
                                     &mut ctx.mcdf16);
                                 self.state_prediction_mode.reset(&mut ctx.m8);
                                 if let DivansOpResult::Failure(_) = ret {
                                     return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::from(ret)));
                                 }
                                 return CodecTraitResult::UpdateCodecTraitAndAdvance(
                                     construct_codec_trait_from_bookkeeping(&mut ctx.lbk));
                             } else {
                                 let pm = core::mem::replace(&mut self.state_prediction_mode,
                                                             context_map::PredictionModeState::<AllocU8>::nop());
                                 self.state_populate_ring_buffer = Command::PredictionMode(pm.pm);
                                 self.state = EncodeOrDecodeState::PopulateRingBuffer;
                             }
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
                            let new_block_type = match self.state_lit_block_switch {
                                block_type::LiteralBlockTypeState::FullyDecoded(btype, stride) => LiteralBlockSwitch::new(btype, stride),
                                _ => return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(
                                    DivansResult::Failure(ErrMsg::UnintendedCodecState(0)))),
                            };
                            self.cross_command_state.bk.obs_btypel(new_block_type);
                            match self.cross_command_state.thread_ctx.lbk() {
                                Some(book_keeping) => {
                                    book_keeping.obs_literal_block_switch(new_block_type);
                                    self.state = EncodeOrDecodeState::Begin;
                                    return CodecTraitResult::Res(OneCommandReturn::Advance);
                                },
                                None => {
                                    self.state_populate_ring_buffer = Command::BlockSwitchLiteral(new_block_type);
                                    self.state = EncodeOrDecodeState::PopulateRingBuffer;                                    
                                },
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
                    let (ret, cmd, _unused) = {
                        let (m8, recoder) = match self.cross_command_state.thread_ctx {
                            ThreadContext::MainThread(ref mut main_thread_ctx) => (Some(&mut main_thread_ctx.m8), Some(&mut main_thread_ctx.recoder)),
                            ThreadContext::Worker => (None, None),
                        };
                        self.cross_command_state.demuxer.push_command(
                            CommandResult::Cmd(core::mem::replace(&mut self.state_populate_ring_buffer,
                                                                  Command::<AllocatedMemoryPrefix<u8, AllocU8>>::nop())),
                            m8,
                            recoder,
                            &mut self.cross_command_state.specialization,
                            output_bytes,
                            output_bytes_offset,
                        )
                    };
                    if let Some(command) = cmd {
                        self.state_populate_ring_buffer = command;
                    }
                    match ret {
                        DivansOutputResult::NeedsMoreOutput => {
                            if Specialization::DOES_CALLER_WANT_ORIGINAL_FILE_BYTES {
                                return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::NeedsMoreOutput)); // we need the caller to drain the buffer
                            }
                        },
                        DivansOutputResult::Failure(m) => {
                            return CodecTraitResult::Res(OneCommandReturn::BufferExhausted(DivansResult::Failure(m)));
                        },
                        DivansOutputResult::Success => {
                            // clobber bk.last_8_literals with the last 8 literals
                            match self.cross_command_state.thread_ctx {
                                ThreadContext::MainThread(ref mut ctx) => {
                                    let last_8 = ctx.recoder.last_8_literals();
                                    ctx.lbk.last_8_literals = //FIXME(threading) only should be run in the main thread
                                        u64::from(last_8[0])
                                        | (u64::from(last_8[1])<<0x8)
                                        | (u64::from(last_8[2])<<0x10)
                                        | (u64::from(last_8[3])<<0x18)
                                        | (u64::from(last_8[4])<<0x20)
                                        | (u64::from(last_8[5])<<0x28)
                                        | (u64::from(last_8[6])<<0x30)
                                        | (u64::from(last_8[7])<<0x38);
                                }
                                ThreadContext::Worker => {}, // Main thread tracks literals
                            }
                            self.state = EncodeOrDecodeState::Begin;
                            return CodecTraitResult::Res(OneCommandReturn::Advance);
                        },
                    }
                },
            }
        }
    }
}

pub fn empty_prediction_mode_context_map<ISl:SliceWrapper<u8>+Default>() -> PredictionModeContextMap<ISl> {
    PredictionModeContextMap::<ISl> {
        literal_context_map:ISl::default(),
        predmode_speed_and_distance_context_map:ISl::default(),
    }
}
