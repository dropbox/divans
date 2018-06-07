pub use ::interface::{ArithmeticEncoderOrDecoder, NewWithAllocator, DivansResult, ReadableBytes, WritableBytes};
use probability::{ProbRange, CDF16, LOG2_SCALE};
use alloc::{SliceWrapper, Allocator};
use brotli;
use codec::CommandArray;
use core;
use codec;
use slice_util::AllocatedMemoryPrefix;
use codec::{EncoderOrDecoderSpecialization};
use mux::{Mux,DevNull};
use codec::io::DemuxerAndRingBuffer;
use cmd_to_divans::EncoderSpecialization;
use brotli::interface::{Command, CopyCommand, Nop, PredictionModeContextMap, LiteralCommand, DictCommand};
use alloc_util;


#[allow(non_camel_case_types)]
type floatY = f32;

use brotli::enc::util::FastLog2u16;

#[derive(Default, Copy, Clone)]
pub struct TallyingArithmeticEncoder {
    snapshot_cost: floatY,
    cost: floatY,
}

impl TallyingArithmeticEncoder {
    pub fn take_snapshot(&mut self) {
        self.snapshot_cost = self.cost;
    }
    pub fn snapshot_delta(&self) -> floatY {
        self.cost - self.snapshot_cost
    }
    pub fn reset_snapshot(&mut self) {
        self.cost = self.snapshot_cost;
        self.snapshot_cost = 0.0;
    }
    pub fn total_cost(&self) ->floatY {
        self.cost
    }
    pub fn tally(&mut self, data: ProbRange) {
        self.cost += LOG2_SCALE as floatY - FastLog2u16(data.freq as u16) as floatY;
    }
}

impl<AllocU8:Allocator<u8>> NewWithAllocator<AllocU8> for TallyingArithmeticEncoder {
    fn new(_m8:&mut AllocU8) -> Self {
        TallyingArithmeticEncoder::default()
    }
    fn free(&mut self, _m8:&mut AllocU8) {}
}

impl ArithmeticEncoderOrDecoder for TallyingArithmeticEncoder {
    fn mov(&mut self) -> Self {
        self.clone()
    }
    fn has_data_to_drain_or_fill(&self) -> bool {
        false
    }
    fn drain_or_fill_internal_buffer_unchecked(&mut self,
                                               _input: &mut ReadableBytes,
                                               _output:&mut WritableBytes) -> DivansResult {
        DivansResult::Success
    }
    fn close(&mut self) -> DivansResult {
        DivansResult::Success
    }
        
    fn get_or_put_bit_without_billing(&mut self,
                                      bit: &mut bool,
                                      prob_of_false: u8) {
        let prob = 
            if *bit {
                i16::from(prob_of_false)
            } else {
                255 - i16::from(prob_of_false)
            };
        let start = if *bit {
            i16::from(prob_of_false)
        } else {
            0
        };
        self.tally(ProbRange{
            start: (start << 7) -1,
            freq: (prob << 7) - 1,
        });
    }
    #[inline(always)]
    fn get_or_put_nibble_without_billing<C: CDF16>(&mut self,
                                                   nibble: &mut u8,
                                                   prob: &C) -> ProbRange {
        let ret = prob.sym_to_start_and_freq(*nibble).range;
        self.tally(ret);
        ret
    }

}

pub fn reset_billing_snapshot<SelectedCDF:CDF16,
                          AllocU8:Allocator<u8>,
                          AllocCDF16:Allocator<SelectedCDF>,
                          >(codec:&mut codec::DivansCodec<TallyingArithmeticEncoder,
                                                          EncoderSpecialization,
                                                          DemuxerAndRingBuffer<AllocU8, DevNull<AllocU8>>,
                                                          DevNull<AllocU8>,
                                                          SelectedCDF,
                                                          AllocU8,
                                                          AllocCDF16>) {
    match codec.cross_command_state.thread_ctx {
        codec::ThreadContext::Worker => {},
        codec::ThreadContext::MainThread(ref mut ctx) => ctx.lit_coder.reset_snapshot(),
    }
    codec.cross_command_state.coder.reset_snapshot()
}

pub fn take_billing_snapshot<SelectedCDF:CDF16,
                          AllocU8:Allocator<u8>,
                          AllocCDF16:Allocator<SelectedCDF>,
                          >(codec:&mut codec::DivansCodec<TallyingArithmeticEncoder,
                                                          EncoderSpecialization,
                                                          DemuxerAndRingBuffer<AllocU8, DevNull<AllocU8>>,
                                                          DevNull<AllocU8>,
                                                          SelectedCDF,
                                                          AllocU8,
                                                          AllocCDF16>) {
    match codec.cross_command_state.thread_ctx {
        codec::ThreadContext::Worker => {},
        codec::ThreadContext::MainThread(ref mut ctx) => ctx.lit_coder.take_snapshot(),
    }
    codec.cross_command_state.coder.take_snapshot()
}

pub fn billing_snapshot_delta<SelectedCDF:CDF16,
                          AllocU8:Allocator<u8>,
                          AllocCDF16:Allocator<SelectedCDF>,
                          >(codec:&codec::DivansCodec<TallyingArithmeticEncoder,
                                                          EncoderSpecialization,
                                                          DemuxerAndRingBuffer<AllocU8, DevNull<AllocU8>>,
                                                          DevNull<AllocU8>,
                                                          SelectedCDF,
                                                          AllocU8,
                                                          AllocCDF16>) -> floatY {
    let mut ret = codec.cross_command_state.coder.snapshot_delta();
    match codec.cross_command_state.thread_ctx {
        codec::ThreadContext::Worker => ret,
        codec::ThreadContext::MainThread(ref ctx) => ret + ctx.lit_coder.snapshot_delta(),
    }
}

pub fn total_billing_cost<SelectedCDF:CDF16,
                          AllocU8:Allocator<u8>,
                          AllocCDF16:Allocator<SelectedCDF>,
                          >(codec:&codec::DivansCodec<TallyingArithmeticEncoder,
                                                          EncoderSpecialization,
                                                          DemuxerAndRingBuffer<AllocU8, DevNull<AllocU8>>,
                                                          DevNull<AllocU8>,
                                                          SelectedCDF,
                                                          AllocU8,
                                                          AllocCDF16>) -> floatY {
    let mut ret = codec.cross_command_state.coder.total_cost();
    match codec.cross_command_state.thread_ctx {
        codec::ThreadContext::Worker => ret,
        codec::ThreadContext::MainThread(ref ctx) => ret + ctx.lit_coder.total_cost(),
    }
}


pub struct OneCommandThawingArray<'a>(pub &'a brotli::interface::Command<brotli::SliceOffset>, pub &'a brotli::InputPair<'a>);

impl<'a> CommandArray for OneCommandThawingArray<'a> {
    fn get_input_command(&self, offset:usize) -> brotli::interface::Command<brotli::InputReference> {
        brotli::thaw_pair(self.0, self.1)
    }
    fn len(&self) -> usize {
        1
    }
}

pub struct ToggleProbabilityBlend {
    pub will_it_blend: bool,
    enc: EncoderSpecialization,
}

impl Default for ToggleProbabilityBlend {
    fn default() -> Self {
        ToggleProbabilityBlend {
            will_it_blend: true,
            enc: EncoderSpecialization::default(),
        }
    }
}

impl EncoderOrDecoderSpecialization for ToggleProbabilityBlend {
    const DOES_CALLER_WANT_ORIGINAL_FILE_BYTES: bool = false;
    const IS_DECODING_FILE: bool = false;
    fn adapt_cdf(&self) -> bool {
        self.will_it_blend
    }
    fn alloc_literal_buffer<AllocU8:Allocator<u8>>(&mut self,
                                                   m8:&mut AllocU8,
                                                   len: usize) -> AllocatedMemoryPrefix<u8, AllocU8> {
        self.enc.alloc_literal_buffer(m8, len)
    }
    fn get_input_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                      data:&'a [Command<ISlice>],
                                                      offset: usize,
                                                      backing:&'a Command<ISlice>) -> &'a Command<ISlice> {
        self.enc.get_input_command(data, offset, backing)
    }
    fn get_output_command<'a, AllocU8:Allocator<u8>>(&self,
                                                     data:&'a mut [Command<AllocatedMemoryPrefix<u8, AllocU8>>],
                                                     offset: usize,
                                                     backing:&'a mut Command<AllocatedMemoryPrefix<u8, AllocU8>>) -> &'a mut Command<AllocatedMemoryPrefix<u8, AllocU8>> {
        self.enc.get_output_command(data, offset, backing)
    }
    fn get_source_copy_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                            data: &'a Command<ISlice>,
                                                            backing: &'a CopyCommand) -> &'a CopyCommand {
        self.enc.get_source_copy_command(data, backing)
    }
    fn get_source_literal_command<'a,
                                  ISlice:SliceWrapper<u8>
                                         +Default>(&self,
                                                   data: &'a Command<ISlice>,
                                                   backing: &'a LiteralCommand<ISlice>) -> &'a LiteralCommand<ISlice> {
        self.enc.get_source_literal_command(data, backing)
    }
    fn get_source_dict_command<'a, ISlice:SliceWrapper<u8>>(&self,
                                                            data: &'a Command<ISlice>,
                                                            backing: &'a DictCommand) -> &'a DictCommand {
        self.enc.get_source_dict_command(data, backing)
    }
    fn get_literal_byte<ISlice:SliceWrapper<u8>>(&self,
                        in_cmd: &LiteralCommand<ISlice>,
                        index: usize) -> u8 {
        self.enc.get_literal_byte(in_cmd, index)
    }
    fn get_recoder_output<'a>(&'a mut self,
                              passed_in_output_bytes: &'a mut [u8]) -> &'a mut[u8] {
        self.enc.get_recoder_output(passed_in_output_bytes)
    }
    fn get_recoder_output_offset<'a>(&self,
                                     passed_in_output_bytes: &'a mut usize,
                                     backing: &'a mut usize) -> &'a mut usize {
        self.enc.get_recoder_output_offset(passed_in_output_bytes, backing)
    }
                          

}
