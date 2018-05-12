// This file contains a threaded decoder
use core;
use interface::{DivansResult, StreamMuxer, StreamDemuxer};
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
    Nop,
    Command,
};

use threading::{MainToThread};
use super::priors::LiteralNibblePriors;
use ::priors::PriorCollection;

pub struct DivansDecoderCodec<Cdf16:CDF16,
                          AllocU8:Allocator<u8>,
                          AllocCDF16:Allocator<Cdf16>,
                          ArithmeticCoder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>,
                          Worker: MainToThread<AllocU8>,
                          LinearInputBytes: StreamDemuxer<AllocU8>> {
    pub ctx: MainThreadContext<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder>,
    pub worker: Worker,
    pub demuxer: LinearInputBytes,
    pub codec_traits: CodecTraitSelector,
    pub crc: SubDigest,
    pub frozen_checksum: Option<u64>,
    pub skip_checksum: bool,
    pub state_lit: LiteralState<AllocU8>,
    pub state_populate_ring_buffer: Command<AllocatedMemoryPrefix<u8, AllocU8>>,
    pub specialization: DecoderSpecialization,
}


impl<Cdf16:CDF16,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<Cdf16>,
     ArithmeticCoder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>,
     Worker: MainToThread<AllocU8>,
     LinearInputBytes: Default+StreamDemuxer<AllocU8>> DivansDecoderCodec<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder, Worker, LinearInputBytes> {
    pub fn new(main_thread_context: MainThreadContext<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder>,
           worker: Worker,
           crc: SubDigest,
           skip_checksum: bool) -> Self {
        let codec_trait = construct_codec_trait_from_bookkeeping(&main_thread_context.lbk);
        DivansDecoderCodec::<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder, Worker, LinearInputBytes> {
            crc:crc,
            skip_checksum:skip_checksum,
            ctx: main_thread_context,
            worker:worker,
            demuxer: LinearInputBytes::default(),
            codec_traits:codec_trait,
            frozen_checksum: None,
            state_lit: LiteralState {
                lc:LiteralCommand::<AllocatedMemoryPrefix<u8, AllocU8>>::nop(),
                state:LiteralSubstate::Begin,
            },
            state_populate_ring_buffer:Command::<AllocatedMemoryPrefix<u8, AllocU8>>::nop(),
            specialization:DecoderSpecialization::default(),
        }
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
