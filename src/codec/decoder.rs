// This file contains a threaded decoder
use core;
use core::hash::Hasher;
use interface::{DivansResult, DivansOutputResult, DivansInputResult, StreamMuxer, StreamDemuxer, StreamID, ErrMsg};
use mux::DevNull;
use ::probability::{CDF16, Speed, ExternalProbCDF16};
use super::priors::{LiteralNibblePriorType, LiteralCommandPriorType, LiteralCMPriorType};
use ::slice_util::{AllocatedMemoryPrefix, AllocatedMemoryRange};
use ::alloc_util::UninitializedOnAlloc;
use ::divans_to_raw::DecoderSpecialization;
use super::literal::{LiteralState, LiteralSubstate};
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use super::crc32::{crc32c_init,crc32c_update};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    ByteContext,
    round_up_mod_4,
    LiteralBookKeeping,
    drain_or_fill_static_buffer,
    MainThreadContext,
    CMD_CODER,
    LIT_CODER,
    DEBUG_TRACK,
};
use super::specializations::{
    construct_codec_trait_from_bookkeeping,
    CodecTraitSelector,
    CodecTraits,
    DEFAULT_TRAIT,
};


use ::interface::{
    NewWithAllocator,
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    LiteralCommand,
    PredictionModeContextMap,
    Nop,
    Command,
    free_cmd,
};

use threading::{MainToThread, CommandResult};
use super::priors::LiteralNibblePriors;
use ::priors::PriorCollection;

pub struct DivansDecoderCodec<Cdf16:CDF16,
                          AllocU8:Allocator<u8>,
                          AllocCDF16:Allocator<Cdf16>,
                          ArithmeticCoder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>,
                          LinearInputBytes: StreamDemuxer<AllocU8>> {
    pub ctx: MainThreadContext<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder>,
    pub demuxer: LinearInputBytes,
    pub devnull: DevNull<AllocU8>,
    pub eof: bool,
    pub nop: LiteralCommand<AllocatedMemoryPrefix<u8, AllocU8>>,
    pub codec_traits: CodecTraitSelector,
    pub crc: SubDigest,
    pub frozen_checksum: Option<u64>,
    pub deserialized_crc:[u8;8],
    pub deserialized_crc_count: u8,
    pub skip_checksum: bool,
    pub state_lit: LiteralState<AllocU8>,
    pub state_populate_ring_buffer: Command<AllocatedMemoryPrefix<u8, AllocU8>>,
    pub specialization: DecoderSpecialization,
    pub outstanding_buffer_count: usize,
    pub doing_ring_buffer_populate: bool,
}


impl<Cdf16:CDF16,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<Cdf16>,
     ArithmeticCoder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>,
     LinearInputBytes: Default+StreamDemuxer<AllocU8>> DivansDecoderCodec<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder, LinearInputBytes> {
    pub fn new(main_thread_context: MainThreadContext<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder>,
           crc: SubDigest,
           skip_checksum: bool) -> Self {
        let codec_trait = construct_codec_trait_from_bookkeeping(&main_thread_context.lbk);
        DivansDecoderCodec::<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder, LinearInputBytes> {
            ctx: main_thread_context,
            demuxer: LinearInputBytes::default(),
            codec_traits:codec_trait,
            frozen_checksum: None,
            state_lit: LiteralState {
                lc:LiteralCommand::<AllocatedMemoryPrefix<u8, AllocU8>>::nop(),
                state:LiteralSubstate::FullyDecoded,
            },
            devnull: DevNull::default(),
            nop: LiteralCommand::<AllocatedMemoryPrefix<u8, AllocU8>>::nop(),
            doing_ring_buffer_populate: false,
            state_populate_ring_buffer:Command::nop(),
            specialization:DecoderSpecialization::default(),
            outstanding_buffer_count: 0,
            deserialized_crc:[0u8;8],
            deserialized_crc_count: 0u8,
            skip_checksum:skip_checksum,
            crc:crc,
            eof:false,
        }
    }
    #[cfg_attr(not(feature="no-inline"), inline(always))]
    pub fn decode_process_input<Worker: MainToThread<AllocU8>>(&mut self,
                                                               worker:&mut Worker,
                                                               input: &[u8],
                                                               input_offset: &mut usize) -> DivansInputResult {
        {
            let adjusted_input_bytes = input.split_at(*input_offset).1;
            let adjusted_input_bytes_offset = self.demuxer.write_linear(
                adjusted_input_bytes,
                self.ctx.m8.get_base_alloc());
            if !self.skip_checksum {
                self.crc.write(adjusted_input_bytes.split_at(adjusted_input_bytes_offset).0);
            }
            *input_offset += adjusted_input_bytes_offset;
        }
        if self.demuxer.encountered_eof() && usize::from(self.deserialized_crc_count) != self.deserialized_crc.len() {
            let crc_bytes_remaining = self.deserialized_crc.len() - usize::from(self.deserialized_crc_count);
            let amt_to_copy = core::cmp::min(input.len() - *input_offset, crc_bytes_remaining);
            self.deserialized_crc.split_at_mut(usize::from(self.deserialized_crc_count)).1.split_at_mut(amt_to_copy).0.clone_from_slice(
                input.split_at(*input_offset).1.split_at(amt_to_copy).0);
            self.deserialized_crc_count += amt_to_copy as u8;
            *input_offset += amt_to_copy;
        }
        // beginning and end??
        let buf_to_push_len;
        {
            let buf_to_push = self.demuxer.edit(CMD_CODER as StreamID);
            buf_to_push_len = buf_to_push.slice().len();
        }
        match worker.push(self.demuxer.edit(CMD_CODER as StreamID)) {
            Ok(_) => {
                self.outstanding_buffer_count += 1;
            },
            Err(_) => {
                if self.outstanding_buffer_count == 0 && self.eof == false && (
                    self.demuxer.data_ready(CMD_CODER as StreamID) != 0 || !self.demuxer.encountered_eof()) {
                    return DivansInputResult::NeedsMoreInput;
                }
            }, // too full
        }
        DivansInputResult::Success
    }
    #[cfg_attr(not(feature="no-inline"), inline(always))]
    fn populate_ring_buffer<Worker:MainToThread<AllocU8>>(&mut self,
                                                          worker: &mut Worker,
                                                          output: &mut [u8],
                                                          output_offset: &mut usize) -> DivansOutputResult {

        match self.ctx.recoder.encode_cmd(&self.state_populate_ring_buffer, output, output_offset) {
            DivansOutputResult::Success => free_cmd(&mut self.state_populate_ring_buffer,
                                                    &mut self.ctx.m8.use_cached_allocation::<
                                                            UninitializedOnAlloc>()),
            DivansOutputResult::Failure(f) => {
                free_cmd(&mut self.state_populate_ring_buffer, &mut self.ctx.m8.use_cached_allocation::<
                        UninitializedOnAlloc>());
                return DivansOutputResult::Failure(f);
            },
            need_something => return need_something,
        }
        DivansOutputResult::Success
    }
    #[cold]
    fn process_eof(&mut self) -> DivansInputResult {
        if usize::from(self.deserialized_crc_count) != self.deserialized_crc.len() {
            return DivansInputResult::NeedsMoreInput;
        }
        let crc = self.crc.finish();
        let checksum = [crc as u8 & 255,
                        (crc >> 8) as u8 & 255,
                        (crc >> 16) as u8 & 255,
                        (crc >> 24) as u8 & 255,
                        b'a',
                        b'n',
                        b's',
                        b'~'];
        for (index, (chk, fil)) in checksum.iter().zip(
            self.deserialized_crc.iter()).enumerate() {
            if *chk != *fil {
                if index >= 4 || !self.skip_checksum {
                    return DivansInputResult::Failure(ErrMsg::BadChecksum(*chk, *fil));
                }
            }
        }
        return DivansInputResult::Success; // DONE decoding
    }
    /*
    fn interpret_thread_literal(&mut self, lit: LiteralCommand<AllocatedMemoryPrefix<u8, AllocU8>>) {
        if let Command::Literal(lit) = cmd {
            DEBUG_TRACK(32);
            let num_bytes = lit.data.1;
            assert_eq!(self.state_lit.lc.data.0.slice().len(), 0);
            self.state_lit.lc = lit;
            self.state_lit.lc.data = self.ctx.m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(num_bytes);
            DEBUG_TRACK(33);
        } else {
            DEBUG_TRACK(34);
            self.state_populate_ring_buffer=Some(cmd);
            DEBUG_TRACK(35);
        }
}*/
    #[cfg_attr(not(feature="no-inline"), inline(always))]
    pub fn decode_process_output<Worker: MainToThread<AllocU8>>(&mut self,
                                                                worker:&mut Worker,
                                                                output: &mut [u8],
                                                                output_offset: &mut usize) -> DecoderResult{
        //{DEBUG_TRACK(18)};
        let mut extra_buffer =  AllocatedMemoryRange::<u8, AllocU8>::default();
        let mut extra_cmd = Command::<AllocatedMemoryPrefix<u8, AllocU8>>::nop();
        loop {
            match self.state_lit.state{
                LiteralSubstate::FullyDecoded => {            /*{DEBUG_TRACK(20)};*/}, // default case--nothing to do here
                _ => {
                    //{DEBUG_TRACK(21)};
                    match self.state_lit.encode_or_decode_content_bytes(
                            self.ctx.m8.get_base_alloc(),
                            &mut self.ctx.lit_coder,
                            &mut self.ctx.lbk,
                            &mut self.ctx.lit_high_priors,
                            &mut self.ctx.lit_low_priors,
                            &mut self.demuxer,
                            &mut self.devnull,
                            &self.nop,
                            output,
                            output_offset,
                            &DEFAULT_TRAIT,
                            &self.specialization) { 
                        DivansResult::Success => {
                            debug_assert!(match self.state_lit.state{LiteralSubstate::FullyDecoded => true, _ => false});
                            self.doing_ring_buffer_populate = true;
                            self.state_populate_ring_buffer = Command::Literal(
                                core::mem::replace(&mut self.state_lit.lc,
                                                   LiteralCommand::<AllocatedMemoryPrefix<u8, AllocU8>>::nop()));
                        },
                        retval => {
                            //{DEBUG_TRACK(22)};
                            return DecoderResult::Processed(retval);
                        }
                    }
                    //{DEBUG_TRACK(23)};
                },
            }
            if self.doing_ring_buffer_populate {
                match self.populate_ring_buffer(worker, output, output_offset) {
                    DivansOutputResult::Success => {
                        self.doing_ring_buffer_populate = false;
                        if Worker::COOPERATIVE_MAIN {
                            return DecoderResult::Yield;
                        }
                    },
                    need_something => return DecoderResult::Processed(DivansResult::from(need_something)),
                }
            }
            if self.eof {
                return DecoderResult::Processed(DivansResult::from(self.process_eof()));
            }
            match worker.pull(&mut extra_cmd, &mut extra_buffer) {
                CommandResult::Eof => {
                    self.eof = true;
                    //{DEBUG_TRACK(1)};
                    return DecoderResult::Processed(DivansResult::from(self.process_eof()));
                },
                CommandResult::Yield => {
                    //{DEBUG_TRACK(3)};
                    assert_eq!(Worker::COOPERATIVE_MAIN, true);
                    return DecoderResult::Yield;
                },
                CommandResult::ProcessedData => {
                    //{DEBUG_TRACK(2)};
                    assert!(extra_buffer.0.slice().len() != 0);

                    //{DEBUG_TRACK(4)};
                    self.outstanding_buffer_count -= 1;
                    let mut need_input = false;
let but_to_push_len;
                    {
                        let buf_to_push = self.demuxer.edit(CMD_CODER as StreamID);
                         but_to_push_len = buf_to_push.0.slice().len();
                    }
                    //{DEBUG_TRACK(5)};
                    match worker.push(self.demuxer.edit(CMD_CODER as StreamID)) {
                        Ok(_) => {
                            //{DEBUG_TRACK(6)};
                            self.outstanding_buffer_count += 1;
                        },
                        Err(_) => {
                            //{DEBUG_TRACK(7)};
                            // this is tricky logic:
                            // if there are no outstanding buffers and we have either not encountered the EOf or still have bytes avail to send
                            // to the cmd stream
                            // then we need to signal to our caller that we need input for the worker
                            if self.outstanding_buffer_count == 0 && self.eof == false && (
                                self.demuxer.data_ready(CMD_CODER as StreamID) != 0 || !self.demuxer.encountered_eof()) {
                                //{DEBUG_TRACK(8)};
                                need_input = true;
                            }
                        },
                    }
                    //{DEBUG_TRACK(9)};
                    let possible_replacement = self.demuxer.edit(CMD_CODER as StreamID);
                    let possible_replacement_len = possible_replacement.0.slice().len();
                    if possible_replacement_len == 0 { // FIXME: do we want to replace, if twice as big?
                        //{DEBUG_TRACK(10)};
                        core::mem::swap(possible_replacement, &mut extra_buffer);
                        possible_replacement.1 = 0..0;
                    } else {
                        //{DEBUG_TRACK(11)};
                        if false && possible_replacement_len * 2 <= extra_buffer.0.slice().len() {
                            extra_buffer.0.slice_mut()[..possible_replacement_len].clone_from_slice(possible_replacement.0.slice());
                            core::mem::swap(&mut possible_replacement.0, &mut extra_buffer.0);
                        }
                        //self.ctx.m8.use_cached_allocation::<UninitializedOnAlloc>().free_cell(AllocatedMemoryPrefix(dat.0, 0));
                        self.ctx.m8.free_cell(core::mem::replace(&mut extra_buffer.0,
                                                                 AllocU8::AllocatedMemory::default()));
                    }
                    if need_input {
                        //{DEBUG_TRACK(12)};
                        return DecoderResult::Processed(DivansResult::NeedsMoreInput);
                    }
                },
                CommandResult::Cmd => {
                    match &mut extra_cmd {
                        &mut Command::Literal(ref lit) => {
                            let num_bytes = lit.data.1;
                            self.state_lit.lc.data = self.ctx.m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(num_bytes);
                            let last_8 = self.ctx.recoder.last_8_literals();
                            self.ctx.lbk.last_8_literals = //FIXME(threading) only should be run in the main thread
                                u64::from(last_8[0])
                                | (u64::from(last_8[1])<<0x8)
                                | (u64::from(last_8[2])<<0x10)
                                | (u64::from(last_8[3])<<0x18)
                                | (u64::from(last_8[4])<<0x20)
                                | (u64::from(last_8[5])<<0x28)
                                | (u64::from(last_8[6])<<0x30)
                                | (u64::from(last_8[7])<<0x38);
                            let new_state = self.state_lit.get_nibble_code_state(0, &self.state_lit.lc, self.demuxer.read_buffer()[LIT_CODER].bytes_avail());
                            self.state_lit.state = new_state;

                        },
                        &mut Command::PredictionMode(ref mut pred_mode) => {
                            let ret = self.ctx.lbk.obs_prediction_mode_context_map(
                                pred_mode,
                                &mut self.ctx.mcdf16);
                            self.codec_traits = construct_codec_trait_from_bookkeeping(&self.ctx.lbk);
                            match worker.push_context_map(core::mem::replace(pred_mode,
                            PredictionModeContextMap::<AllocatedMemoryPrefix<u8, AllocU8>> {
                                literal_context_map:AllocatedMemoryPrefix::<u8, AllocU8>::default(),
                                predmode_speed_and_distance_context_map:AllocatedMemoryPrefix::<u8, AllocU8>::default(),
                            })) {
                                Ok(_) => {},
                                Err(_) => panic!("thread unalbe to accept 2 concurrent context map"),
                            }
                        }
                        &mut Command::BlockSwitchLiteral(ref new_block_type) => {
                            self.ctx.lbk.obs_literal_block_switch(new_block_type.clone());
                            self.codec_traits = construct_codec_trait_from_bookkeeping(&self.ctx.lbk);
                        },
                        remainder => {
                            self.doing_ring_buffer_populate = true;
                            core::mem::swap(&mut self.state_populate_ring_buffer, remainder);
                        },
                    }
                },
            }
            //{DEBUG_TRACK(15)};
        }
        //{DEBUG_TRACK(16)};
    }
    pub fn decode<Worker: MainToThread<AllocU8>>(&mut self,
                                                 worker:&mut Worker,
                                                 input: &[u8],
                                                 input_offset: &mut usize,
                                                 output: &mut [u8],
                                                 output_offset: &mut usize) -> DivansResult {
        match self.decode_process_input(worker, input, input_offset) {
            DivansInputResult::Success => {},
            need_something => return DivansResult::from(need_something),
        }
        match self.decode_process_output(worker, output, output_offset) {
            DecoderResult::Processed(retval) => retval,
            DecoderResult::Yield => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
#[inline(always)]
pub fn default_crc() -> SubDigest {
    SubDigest(crc32c_init())
}

impl Default for SubDigest {
    #[inline(always)]
    fn default() -> Self {
        default_crc()
    }
}
pub enum DecoderResult {
    Processed(DivansResult),
    Yield,
}
