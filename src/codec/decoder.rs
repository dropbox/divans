// This file contains a threaded decoder
use core;
use core::hash::Hasher;
use interface::{DivansOpResult, DivansResult, DivansOutputResult, DivansInputResult, StreamDemuxer, StreamID, ErrMsg};
use mux::DevNull;
use ::probability::{CDF16};
use ::slice_util::{AllocatedMemoryPrefix, AllocatedMemoryRange};
use ::alloc_util::UninitializedOnAlloc;
use ::divans_to_raw::DecoderSpecialization;
use super::literal::{LiteralState, LiteralSubstate};
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use super::crc32::{crc32c_init,crc32c_update};
use super::interface::{
    MainThreadContext,
    StructureSeeker,
    CMD_CODER,
    LIT_CODER,
};
use super::specializations::{
    construct_codec_trait_from_bookkeeping,
    CodecTraitSelector,
};


use ::interface::{
    NewWithAllocator,
    ArithmeticEncoderOrDecoder,
    LiteralCommand,
    PredictionModeContextMap,
    Nop,
    Command,
    
    free_cmd,
};

use threading::{MainToThread, PullAllocatedCommand, CommandResult, NUM_SERIAL_COMMANDS_BUFFERED, StaticCommand};

pub struct DivansDecoderCodec<Cdf16:CDF16,
                              AllocU8:Allocator<u8>,
                              AllocCDF16:Allocator<Cdf16>,
                              AllocCommand:Allocator<StaticCommand>,
                              ArithmeticCoder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>,
                              Parser:StructureSeeker,
                              LinearInputBytes: StreamDemuxer<AllocU8>> {
    pub ctx: MainThreadContext<Cdf16, AllocU8, AllocCDF16, Parser, ArithmeticCoder>,
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
    pub is_populating_ring_buffer: bool,
    pub state_populate_ring_buffer: Command<AllocatedMemoryPrefix<u8, AllocU8>>,
    pub specialization: DecoderSpecialization,
    pub outstanding_buffer_count: usize,
    pub cmd_buffer: AllocatedMemoryPrefix<StaticCommand, AllocCommand>,
    pub cmd_buffer_offset: usize,
    pub cmd_buffer_contains_eof: bool,
    pub pred_buffer: [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>;2],
}

impl<Cdf16:CDF16,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<Cdf16>,
     AllocCommand:Allocator<StaticCommand>,
     Parser: StructureSeeker,
     ArithmeticCoder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>,
     LinearInputBytes: Default+StreamDemuxer<AllocU8>,
     > DivansDecoderCodec<Cdf16, AllocU8, AllocCDF16, AllocCommand, ArithmeticCoder, Parser, LinearInputBytes> {
    pub fn new(main_thread_context: MainThreadContext<Cdf16, AllocU8, AllocCDF16, Parser, ArithmeticCoder>,
               mcommand: &mut AllocCommand,
               crc: SubDigest,
               skip_checksum: bool) -> Self {
        let codec_trait = construct_codec_trait_from_bookkeeping(&main_thread_context.lbk);
        DivansDecoderCodec::<Cdf16, AllocU8, AllocCDF16, AllocCommand, ArithmeticCoder, Parser, LinearInputBytes> {
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
            is_populating_ring_buffer:false,
            state_populate_ring_buffer:Command::nop(),
            specialization:DecoderSpecialization::default(),
            outstanding_buffer_count: 0,
            deserialized_crc:[0u8;8],
            deserialized_crc_count: 0u8,
            skip_checksum:skip_checksum,
            crc:crc,
            eof:false,
            cmd_buffer_offset:0,
            cmd_buffer:AllocatedMemoryPrefix::realloc(mcommand.alloc_cell(NUM_SERIAL_COMMANDS_BUFFERED),0),
            cmd_buffer_contains_eof:false,
            pred_buffer: [empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>(),
                          empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>()],
        }
    }
    pub fn free(&mut self, mcommand: &mut AllocCommand) {
        mcommand.free_cell(core::mem::replace(&mut self.cmd_buffer.0,
                                              AllocCommand::AllocatedMemory::default()));
        self.ctx.m8.get_base_alloc().free_cell(core::mem::replace(&mut self.state_lit.lc.data.0,
                                                                  AllocU8::AllocatedMemory::default()));
        self.demuxer.free_demux(self.ctx.m8.get_base_alloc());
        for item in self.pred_buffer.iter_mut() {
            free_cmd(&mut Command::PredictionMode(core::mem::replace(item,
                                                                     empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>())),
                     &mut self.ctx.m8.use_cached_allocation::<UninitializedOnAlloc>());
        }
        self.ctx.m8.free_ref();
    }
    pub fn commands_or_data_to_receive(&self) -> bool {
        self.outstanding_buffer_count > 0 || ( // if we have outstanding buffer
            self.demuxer.encountered_eof() && self.demuxer.data_ready(CMD_CODER as StreamID) == 0) // or we have flushed everything we will have
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
        if self.demuxer.data_ready(CMD_CODER as StreamID) != 0 {
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
        } else {
            if self.demuxer.encountered_eof() || self.outstanding_buffer_count > 0 {
                DivansInputResult::Success
            } else {
                DivansInputResult::NeedsMoreInput
            }
        }
    }
    #[cfg_attr(not(feature="no-inline"), inline(always))]
    fn populate_ring_buffer(&mut self,
                            output: &mut [u8],
                            output_offset: &mut usize) -> DivansOutputResult {
        if !self.is_populating_ring_buffer {
            return DivansOutputResult::Success;
        }
        let lbk = &mut self.ctx.lbk;
        match self.ctx.recoder.encode_cmd(&mut self.state_populate_ring_buffer, output, output_offset, &mut |data|lbk.parser_update(data)) {
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
        self.is_populating_ring_buffer = false;
        DivansOutputResult::Success
    }
    #[cold]
    fn process_eof(&mut self, output: &mut [u8], output_offset: &mut usize) -> DivansResult {
        match self.ctx.recoder.flush(output, output_offset) {
            DivansOutputResult::Success => {},
            need_something => return DivansResult::from(need_something),
        }

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
    pub fn decode_process_output<Worker: MainToThread<AllocU8>+PullAllocatedCommand<AllocU8, AllocCommand>>(&mut self,
                                                                worker:&mut Worker,
                                                                output: &mut [u8],
                                                                output_offset: &mut usize) -> DecoderResult{
        //{DEBUG_TRACK(18)};
        loop {
            match self.state_lit.state{
                LiteralSubstate::FullyDecoded => {            /*{DEBUG_TRACK(20)};*/}, // default case--nothing to do here
                _ => {
                    //{DEBUG_TRACK(21)};
                    match match self.codec_traits {
                        CodecTraitSelector::DefaultTrait(tr) =>
                            self.state_lit.encode_or_decode_content_bytes(
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
                                tr,
                                &self.specialization),
                        CodecTraitSelector::MixingTrait(mtr) =>
                            self.state_lit.encode_or_decode_content_bytes(
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
                                mtr,
                                &self.specialization),                            
                    } { 
                                DivansResult::Success => {
                                    assert!(match self.state_lit.state{LiteralSubstate::FullyDecoded => true, _ => false});
                                    self.is_populating_ring_buffer = true;
                                    self.state_populate_ring_buffer = Command::Literal(
                                        core::mem::replace(&mut self.state_lit.lc,
                                                           LiteralCommand::<AllocatedMemoryPrefix<u8, AllocU8>>::nop()));
                                },
                                retval => {
                                    return DecoderResult::Processed(retval);
                                }
                    }
                },
            }
            match self.populate_ring_buffer(output, output_offset) {
                DivansOutputResult::Success => {},
                need_something => return DecoderResult::Processed(DivansResult::from(need_something)),
            }
            if self.eof {
                return DecoderResult::Processed(self.process_eof(output, output_offset));
            }
            if self.cmd_buffer_offset >= self.cmd_buffer.1 as usize && !self.cmd_buffer_contains_eof {
                self.cmd_buffer_offset = 0;
                self.cmd_buffer.1 = 0; //reset the command buffer to zero
                let mut consumed_data = [AllocatedMemoryRange::<u8, AllocU8>::default(),
                                         AllocatedMemoryRange::<u8, AllocU8>::default()];
                let status;
                {
                    assert_eq!(self.pred_buffer[0].has_context_speeds(), false);
                    assert_eq!(self.pred_buffer[1].has_context_speeds(), false);
                    status = worker.pull_command_buf(&mut self.cmd_buffer, &mut consumed_data, &mut self.pred_buffer);
                }
                let mut need_input = false;
                for dat in consumed_data.iter_mut() {
                    if dat.0.len() == 0 {
                        continue; //FIXME: should we yield here?
                        // assert_eq!(Worker::COOPERATIVE_MAIN, true);
                    }
                    self.outstanding_buffer_count -= 1;
                    match worker.push(self.demuxer.edit(CMD_CODER as StreamID)) {
                        Ok(_) => {
                            self.outstanding_buffer_count += 1;
                        },
                        Err(_) => {
                            // this is tricky logic:
                            // if there are no outstanding buffers and we have either not encountered the EOf or still have bytes avail to send
                            // to the cmd stream
                            // then we need to signal to our caller that we need input for the worker
                            if self.outstanding_buffer_count == 0 && self.eof == false && (
                                self.demuxer.data_ready(CMD_CODER as StreamID) != 0 || !self.demuxer.encountered_eof()) {
                                need_input = true;
                            }
                        },
                    }
                    let possible_replacement = self.demuxer.edit(CMD_CODER as StreamID);
                    let possible_replacement_len = possible_replacement.0.slice().len();
                    if possible_replacement_len == 0 { // FIXME: do we want to replace, if twice as big?
                        core::mem::swap(&mut possible_replacement.0, &mut dat.0);
                    } else {
                        if false && possible_replacement_len * 2 <= dat.0.slice().len() {
                            dat.0.slice_mut()[..possible_replacement_len].clone_from_slice(possible_replacement.0.slice());
                            core::mem::swap(&mut possible_replacement.0, &mut dat.0);
                        }
                        //self.ctx.m8.use_cached_allocation::<UninitializedOnAlloc>().free_cell(AllocatedMemoryPrefix(dat.0, 0));
                        self.ctx.m8.free_cell(core::mem::replace(&mut dat.0, AllocU8::AllocatedMemory::default()));
                    }
                }
                match status {
                    CommandResult::Ok => {},
                    CommandResult::Err(e) => return DecoderResult::Processed(DivansResult::Failure(e)),
                    CommandResult::Eof => self.cmd_buffer_contains_eof = true,
                }
                if need_input {
                    return DecoderResult::Processed(DivansResult::NeedsMoreInput);
                }
            }
            if self.cmd_buffer_offset >= self.cmd_buffer.1 as usize{
                if self.cmd_buffer_contains_eof {
                    return DecoderResult::Processed(self.process_eof(output, output_offset));
                } else {
                    if Worker::COOPERATIVE_MAIN  {
                        return DecoderResult::Yield;
                    }
                    continue;
                }
            }
            let offt = self.cmd_buffer_offset;
            let cur_cmd = &mut self.cmd_buffer.slice_mut()[offt];
            self.cmd_buffer_offset += 1;
            if let &mut Command::Copy(cp) = cur_cmd {
                self.is_populating_ring_buffer = true;
                self.state_populate_ring_buffer=Command::Copy(cp);
            } else if let &mut Command::Literal(ref lit) = cur_cmd {
                let num_bytes = lit.data.len();
                self.state_lit.lc.data = self.ctx.m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(num_bytes);
                let (p0, p1) = self.ctx.lbk.parser_prior();
                self.ctx.lbk.last_8_literals =
                    (u64::from(p1)<<0x30) |
                    (u64::from(p0)<<0x38);
                let new_state = self.state_lit.get_nibble_code_state(0, &self.state_lit.lc, self.demuxer.read_buffer()[LIT_CODER].bytes_avail());
                self.state_lit.state = new_state;
                if Worker::COOPERATIVE_MAIN {
                    return DecoderResult::Yield;
                }
            } else {
                match cur_cmd {
                    &mut Command::PredictionMode(_) => {
                    let mut pred_mode = empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>();
                        core::mem::swap(&mut pred_mode, &mut self.pred_buffer[1]);
                        core::mem::swap(&mut pred_mode, &mut self.pred_buffer[0]); // shift pred_buffer[1] to pred_buffer[0] and extract [0]
                        
                        let ret = self.ctx.lbk.obs_prediction_mode_context_map(
                            &pred_mode,
                            &mut self.ctx.mcdf16);
                        match ret {
                            DivansOpResult::Success => {},
                            _ => return DecoderResult::Processed(DivansResult::from(ret)),
                        }
                        self.codec_traits = construct_codec_trait_from_bookkeeping(&self.ctx.lbk);
                        match worker.push_context_map(pred_mode) {
                            Ok(_) => {},
                            Err(_) => panic!("thread unable to accept 2 concurrent context map"),
                        }
                    },
                    &mut Command::BlockSwitchLiteral(new_block_type) => {
                        self.ctx.lbk.obs_literal_block_switch(new_block_type.clone());
                        self.codec_traits = construct_codec_trait_from_bookkeeping(&self.ctx.lbk);
                    },
                    &mut Command::BlockSwitchCommand(mcc) => {
                        self.is_populating_ring_buffer = true;
                        self.state_populate_ring_buffer=Command::BlockSwitchCommand(mcc);
                    },
                    &mut Command::BlockSwitchDistance(mcc) =>{
                        self.is_populating_ring_buffer = true;
                        self.state_populate_ring_buffer=Command::BlockSwitchDistance(mcc);
                    },
                    &mut Command::Dict(dc) => {
                        self.is_populating_ring_buffer = true;
                        self.state_populate_ring_buffer=Command::Dict(dc);
                    },  
                    &mut Command::Literal(_) | &mut Command::Copy(_) => unreachable!(),
                }
            }
        }
    }
    pub fn decode<Worker: MainToThread<AllocU8> + PullAllocatedCommand<AllocU8, AllocCommand> >(&mut self,
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
pub fn empty_prediction_mode_context_map<ISl:SliceWrapper<u8>+Default>() -> PredictionModeContextMap<ISl> {
    PredictionModeContextMap::<ISl> {
        literal_context_map:ISl::default(),
        predmode_speed_and_distance_context_map:ISl::default(),
    }
}

pub enum DecoderResult {
    Processed(DivansResult),
    Yield,
}
