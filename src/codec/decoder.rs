// This file contains a threaded decoder
use core;
use core::hash::Hasher;
use interface::{DivansResult, DivansOutputResult, DivansInputResult, StreamMuxer, StreamDemuxer, StreamID, ErrMsg};
use ::probability::{CDF16, Speed, ExternalProbCDF16};
use super::priors::{LiteralNibblePriorType, LiteralCommandPriorType, LiteralCMPriorType};
use ::slice_util::AllocatedMemoryPrefix;
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
};
use super::specializations::{
    construct_codec_trait_from_bookkeeping,
    CodecTraitSelector,
    CodecTraits,
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
    pub codec_traits: CodecTraitSelector,
    pub crc: SubDigest,
    pub frozen_checksum: Option<u64>,
    pub deserialized_crc:[u8;8],
    pub deserialized_crc_count: u8,
    pub skip_checksum: bool,
    pub state_lit: LiteralState<AllocU8>,
    pub state_populate_ring_buffer: Option<Command<AllocatedMemoryPrefix<u8, AllocU8>>>,
    pub specialization: DecoderSpecialization,
    pub outstanding_buffer_count: usize,
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
            state_populate_ring_buffer:None,
            specialization:DecoderSpecialization::default(),
            outstanding_buffer_count: 0,
            deserialized_crc:[0u8;8],
            deserialized_crc_count: 0u8,
            skip_checksum:skip_checksum,
            crc:crc,
        }
    }
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
            *input_offset += amt_to_copy;
        }
        // beginning and end??
        match worker.push(self.demuxer.edit(CMD_CODER as StreamID)) {
            Ok(_) => self.outstanding_buffer_count += 1,
            Err(_) => {}, // too full
        }
        DivansInputResult::Success
    }
    fn populate_ring_buffer<Worker:MainToThread<AllocU8>>(&mut self,
                                                          worker: &mut Worker,
                                                          output: &mut [u8],
                                                          output_offset: &mut usize) -> DivansOutputResult {
        
        if let Some(ref mut pop_cmd) = self.state_populate_ring_buffer {
            match self.ctx.recoder.encode_cmd(pop_cmd, output, output_offset) {
                DivansOutputResult::Success => {},
                DivansOutputResult::Failure(f) => {
                    free_cmd(pop_cmd, &mut self.ctx.m8.use_cached_allocation::<
                            UninitializedOnAlloc>());
                    return DivansOutputResult::Failure(f);
                },
                need_something => return need_something,
            }
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
        }
        if let Some(mut pop_cmd) = self.state_populate_ring_buffer.take() {
            if let Command::PredictionMode(pred_mode) = pop_cmd {
                match worker.push_context_map(pred_mode) {
                    Ok(_) => {},
                    Err(_) => panic!("thread unalbe to accept 2 concurrent context map"),
                }
            } else {
                free_cmd(&mut pop_cmd, &mut self.ctx.m8.use_cached_allocation::<
                        UninitializedOnAlloc>());
            }
        }

        self.state_populate_ring_buffer = None; // we processed any leftover ringbuffer command
        DivansOutputResult::Success
    }

    pub fn decode_process_output<Worker: MainToThread<AllocU8>>(&mut self,
                                                                worker:&mut Worker,
                                                                output: &mut [u8],
                                                                output_offset: &mut usize) -> DivansResult{
        loop {
            if let LiteralSubstate::FullyDecoded = self.state_lit.state {
            } else { // we have literal to decode
                //FIXME
            }
            match self.populate_ring_buffer(worker, output, output_offset) {
                Success => {},
                need_something => return DivansResult::from(need_something),
            }
            match worker.pull() {
                CommandResult::Eof => {
                    if usize::from(self.deserialized_crc_count) != self.deserialized_crc.len() {
                        return DivansResult::NeedsMoreInput;
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
                                return DivansResult::Failure(ErrMsg::BadChecksum(*chk, *fil));
                            }
                        }
                    }
                    return DivansResult::Success; // DONE decoding
                },
                CommandResult::ProcessedData(mut dat) => {
                    self.outstanding_buffer_count -= 1;
                    let mut need_input = false;
                    match worker.push(self.demuxer.edit(CMD_CODER as StreamID)) {
                        Ok(_) => self.outstanding_buffer_count += 1,
                        Err(_) => {
                            if self.outstanding_buffer_count == 0 && !self.demuxer.encountered_eof() {
                                need_input = true;
                            }
                        },
                    }
                    let possible_replacement = self.demuxer.edit(CMD_CODER as StreamID);
                    let possible_replacement_len = possible_replacement.0.slice().len();
                    if possible_replacement_len == 0 { // FIXME: do we want to replace, if twice as big?
                        core::mem::replace(&mut possible_replacement.0, dat.0);
                    } else {
                        if possible_replacement_len * 2 <= dat.0.slice().len() {
                            dat.0.slice_mut()[..possible_replacement_len].clone_from_slice(possible_replacement.0.slice());
                            let tmp = core::mem::replace(&mut possible_replacement.0, dat.0);
                            dat.0 = tmp;
                        }
                        self.ctx.m8.free_cell(dat.0)
                    }
                    if need_input {
                        return DivansResult::NeedsMoreInput;
                    }
                },
                CommandResult::Cmd(cmd) => {
                    if let Command::Literal(lit) = cmd {
                        let num_bytes = lit.data.1;
                        assert_eq!(self.state_lit.lc.data.0.slice().len(), 0);
                        self.state_lit.lc = lit;
                        self.state_lit.lc.data = self.ctx.m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(num_bytes);
                        let new_state = self.state_lit.get_nibble_code_state(0, &self.state_lit.lc, self.demuxer.read_buffer()[LIT_CODER].bytes_avail());
                        self.state_lit.state = new_state;
                    } else {
                        self.state_populate_ring_buffer=Some(cmd);
                    }
                },
            }
        }
        DivansResult::Success
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
        self.decode_process_output(worker, output, output_offset)
    }
}

#[derive(Clone, Copy, Debug)]
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

impl Default for SubDigest {
    fn default() -> Self {
        default_crc()
    }
}
