// This file contains a threaded decoder
use core;
use interface::{DivansResult, StreamMuxer, StreamDemuxer};
use ::probability::{CDF16, Speed, ExternalProbCDF16};
use super::priors::{LiteralNibblePriorType, LiteralCommandPriorType, LiteralCMPriorType};
use ::slice_util::AllocatedMemoryPrefix;
use ::alloc_util::UninitializedOnAlloc;
use ::divans_to_raw::DecoderSpecialization;
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    ByteContext,
    round_up_mod_4,
    LiteralBookKeeping,
    LIT_CODER,
    CMD_CODER,
    drain_or_fill_static_buffer,
    MainThreadContext,
};

use super::specializations::{CodecTraits};
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    LiteralCommand,
};

use threading::{MainToThread, ThreadToMain};
use super::priors::LiteralNibblePriors;
use ::priors::PriorCollection;
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LiteralSubstate {
    Begin,
    LiteralCountSmall(bool),
    LiteralCountFirst,
    LiteralCountLengthGreater14Less25,
    LiteralCountMantissaNibbles(u8, u32),
    LiteralNibbleIndex(u32),
    SafeLiteralNibbleIndex(u32),
    LiteralNibbleLowerHalf(u32),
    LiteralNibbleIndexWithECDF(u32),
    FullyDecoded,
}


struct DivansDecoderCodec<Cdf16:CDF16,
                          AllocU8:Allocator<u8>,
                          AllocCDF16:Allocator<Cdf16>,
                          ArithmeticCoder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>,
                          Worker: MainToThread<AllocU8>,
                          LinearInputBytes: StreamDemuxer<AllocU8>> {
    ctx: MainThreadContext<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder>,
    worker: Worker,
    demuxer: LinearInputBytes,
    coder: ArithmeticCoder,
    codec_traits: CodecTraitSelector,
    crc: SubDigest,
    frozen_checksum: Option<u64>,
    skip_checksum: bool,
    state_lit: literal::LiteralState<AllocU8>,
    state_populate_ring_buffer: Command<AllocatedMemoryPrefix<u8, AllocU8>>,
    specialization: DecoderSpecialization,
}


