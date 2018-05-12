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

use alloc::{SliceWrapper, Allocator};
use super::slice_util;
use super::probability::interface::{CDF16, ProbRange};
use super::probability;
use super::codec::copy::CopySubstate;
use super::codec::dict::DictSubstate;
use super::codec::literal::LiteralSubstate;
use super::codec::context_map::PredictionModeSubstate;
use super::codec::block_type::BlockTypeState;
pub use super::codec::StrideSelection;
pub use brotli::enc::interface::*;

#[derive(Copy,Clone,Debug)]
pub enum ErrMsg {
    PredictionModeFail(()),
    ShutdownCoderNeedsInput,
    EncodeOneCommandNeedsInput,
    NotAllowedToFlushIfPreviousCommandPartial,
    NotAllowedToEncodeAfterFlush,
    Distance0NotAllowed,
    DrainOrFillNeedsInput(u8),
    BrotliIrGenFlushStreamNeedsInput,
    AssemblerStreamReportsDone,
    UnexpectedEof,
    TrailingInput(u8),
    InputChangedAfterContinuation,
    DistanceGreaterRingBuffer,
    DictTransformDiffersFromExpectedSize,
    MinLogicError,
    InputOffsetOutOfBounds,
    CommandCodeOutOfBounds(u8),
    CopyDistanceMnemonicCodeBad(u8, u8),
    BadChecksum(u8, u8),
    IndexBeyondContextMapSize(u8, u8),
    PredictionModeOutOfBounds(u8),
    DictWordSizeTooLarge(u8),
    DictTransformIndexUndefined(u8),
    BrotliCompressStreamFail(u8, u8),
    BrotliInternalEncodeStreamNeedsOutputWithoutFlush,
    MagicNumberWrongA(u8, u8),
    MagicNumberWrongB(u8, u8),
    BadWindowSize(u8),
    MissingAllocator(u8),
    WrongInternalDecoderState,
    WrongInternalEncoderState(u8),
    UnintendedCodecState(u8),
    MainFunctionCalledFromThread(u8),
}


#[derive(Copy,Clone,Debug)]
pub enum DivansOpResult {
    Failure(ErrMsg),
    Success,
}

impl From<DivansOpResult> for DivansResult {
    fn from(res: DivansOpResult) -> Self {
        match res {
            DivansOpResult::Failure(x) => DivansResult::Failure(x),
            DivansOpResult::Success => DivansResult::Success,
        }
    }
}

impl From<DivansOpResult> for DivansInputResult {
    fn from(res: DivansOpResult) -> Self {
        match res {
            DivansOpResult::Failure(x) => DivansInputResult::Failure(x),
            DivansOpResult::Success => DivansInputResult::Success,
        }
    }
}

impl From<DivansOpResult> for DivansOutputResult {
    fn from(res: DivansOpResult) -> Self {
        match res {
            DivansOpResult::Failure(x) => DivansOutputResult::Failure(x),
            DivansOpResult::Success => DivansOutputResult::Success,
        }
    }
}

#[derive(Copy,Clone,Debug)]
pub enum DivansResult {
    Failure(ErrMsg),
    Success,
    NeedsMoreInput,
    NeedsMoreOutput,
}


#[derive(Copy,Clone,Debug)]
pub enum DivansInputResult {
    Failure(ErrMsg),
    Success,
    NeedsMoreInput,
}
impl From<DivansInputResult> for DivansResult {
    fn from(res: DivansInputResult) -> Self {
        match res {
            DivansInputResult::Failure(x) => DivansResult::Failure(x),
            DivansInputResult::Success => DivansResult::Success,
            DivansInputResult::NeedsMoreInput => DivansResult::NeedsMoreInput,
        }
    }
}
#[derive(Copy,Clone,Debug)]
pub enum DivansOutputResult {
    Failure(ErrMsg),
    Success,
    NeedsMoreOutput,
}
impl From<DivansOutputResult> for DivansResult {
    fn from(res: DivansOutputResult) -> Self {
        match res {
            DivansOutputResult::Failure(x) => DivansResult::Failure(x),
            DivansOutputResult::Success => DivansResult::Success,
            DivansOutputResult::NeedsMoreOutput => DivansResult::NeedsMoreOutput,
        }
    }
}

// The choice of CDF16 struct is controlled by feature flags.
#[cfg(feature="blend")]
pub type DefaultInternalCDF16 = probability::BlendCDF16;
#[cfg(all(not(any(feature="blend")), feature="uncached_frequentist"))]
pub type DefaultInternalCDF16 = probability::FrequentistCDF16;
#[cfg(all(not(any(feature="blend", feature="uncached_frequentist")), feature="simd"))]
pub type DefaultInternalCDF16 = probability::SIMDFrequentistCDF16;
#[cfg(all(not(any(feature="blend", feature="uncached_frequentist", feature="simd"))))]
pub type DefaultInternalCDF16 = probability::OptFrequentistCDF16;

#[cfg(feature="debug_entropy")]
#[cfg(not(feature="findspeed"))]
pub type DefaultCDF16 = probability::DebugWrapperCDF16<DefaultInternalCDF16>;
#[cfg(not(any(feature="debug_entropy", feature="findspeed")))]
pub type DefaultCDF16 = DefaultInternalCDF16;
#[cfg(feature="findspeed")]
pub type DefaultCDF16 = probability::VariantSpeedCDF<DefaultInternalCDF16>;

pub const HEADER_LENGTH: usize = 16;
pub const MAGIC_NUMBER:[u8;4] = [0xff, 0xe5,0x8c, 0x9f];

// Commands that can instantiate as a no-op should implement this.
/*
#[derive(Debug)]
pub struct LiteralCommand<SliceType:SliceWrapper<u8>> {
    pub data: SliceType,
    pub prob: SliceType,
}

impl<SliceType:SliceWrapper<u8>+Default> Nop<LiteralCommand<SliceType>> for LiteralCommand<SliceType> {
    fn nop() -> Self {
        LiteralCommand {
            data: SliceType::default(),
            prob: SliceType::default(),
        }
    }
}

impl<SliceType:SliceWrapper<u8>+Default+Clone> Clone for LiteralCommand<SliceType> {
    fn clone(&self) -> Self {
        LiteralCommand {
            data: self.data.clone(),
            prob: self.prob.clone(),
        }
    }
}
impl<SliceType:SliceWrapper<u8>+Default+Clone+Copy> Copy for LiteralCommand<SliceType> {
}


#[derive(Debug)]
pub enum Command<SliceType:SliceWrapper<u8> > {
    Copy(CopyCommand),
    Dict(DictCommand),
    Literal(LiteralCommand<SliceType>),
    BlockSwitchCommand(BlockSwitch),
    BlockSwitchLiteral(LiteralBlockSwitch),
    BlockSwitchDistance(BlockSwitch),
    PredictionMode(PredictionModeContextMap<SliceType>),
}
impl<SliceType:SliceWrapper<u8>+Default+Clone+Copy> Copy for Command<SliceType> {
}
impl<SliceType:SliceWrapper<u8>+Default+Clone> Clone for Command<SliceType> {
    fn clone(&self) -> Command<SliceType> {
        match self {
            &Command::Copy(ref cpy) => {
                Command::Copy(cpy.clone())
            },
            &Command::Dict(ref dict) => {
                Command::Dict(dict.clone())
            },
            &Command::Literal(ref lit) => {
                Command::Literal(lit.clone())
            },
            &Command::PredictionMode(ref lit) => {
                Command::PredictionMode(lit.clone())
            },
            &Command::BlockSwitchCommand(ref bs) => {
                Command::BlockSwitchCommand(bs.clone())
            },
            &Command::BlockSwitchLiteral(ref bs) => {
                Command::BlockSwitchLiteral(bs.clone())
            },
            &Command::BlockSwitchDistance(ref bs) => {
                Command::BlockSwitchDistance(bs.clone())
            },
        }
    }
}
 */
pub const NUM_STREAMS: usize = 2;
pub const STREAM_ID_MASK: StreamID = 0x1;
pub type StreamID = u8;

pub struct ReadableBytes<'a> {
    pub data: &'a [u8],
    pub read_offset: &'a mut usize,
}

impl<'a> ReadableBytes<'a> {
    pub fn bytes_avail(&self) -> usize {
        self.data.len() - *self.read_offset
    }
}
pub struct WritableBytes<'a> {
    pub data: &'a mut [u8],
    pub write_offset: &'a mut usize,
}

pub trait StreamMuxer<AllocU8: Allocator<u8> > {
    #[inline(always)]
    fn write(&mut self, stream_id: StreamID, data:&[u8], m8: &mut AllocU8) -> usize;
    #[inline(always)]
    fn write_buffer(&mut self, m8: &mut AllocU8) -> [WritableBytes; NUM_STREAMS];
    #[inline(always)]
    fn can_linearize() ->  bool {true}
    #[inline(always)]
    fn linearize(&mut self, output:&mut[u8]) -> usize;
    #[inline(always)]
    fn flush(&mut self, output:&mut[u8]) -> usize;
    #[inline(always)]
    fn wrote_eof(&self) -> bool;
    #[inline(always)]
    fn free_mux(&mut self, m8: &mut AllocU8);
}
pub trait StreamDemuxer<AllocU8: Allocator<u8> > {
    #[inline(always)]
    fn write_linear(&mut self, data:&[u8], m8: &mut AllocU8) -> usize;
    #[inline(always)]
    fn read_buffer(&mut self) -> [ReadableBytes; NUM_STREAMS];
    #[inline(always)]
    fn data_ready(&self, stream_id:StreamID) -> usize;
    #[inline(always)]
    fn peek(&self, stream_id: StreamID) -> &[u8];
    #[inline(always)]
    fn pop(&mut self, stream_id: StreamID) -> slice_util::AllocatedMemoryRange<u8, AllocU8>;
    #[inline(always)]
    fn consume(&mut self, stream_id: StreamID, count: usize);
    #[inline(always)]
    fn encountered_eof(&self) -> bool;
    #[inline(always)]
    fn free_demux(&mut self, m8: &mut AllocU8);
}

pub trait Compressor {
    fn encode(&mut self,
              input:&[u8],
              input_offset: &mut usize,
              output:&mut[u8],
              output_offset:&mut usize) -> DivansResult;
    fn encode_commands<SliceType:SliceWrapper<u8>+Default>(&mut self,
                                          input:&[Command<SliceType>],
                                          input_offset : &mut usize,
                                          output :&mut[u8],
                                          output_offset: &mut usize) -> DivansOutputResult;
    fn flush(&mut self,
                                          output :&mut[u8],
                                          output_offset: &mut usize) -> DivansOutputResult;
}

pub trait Decompressor {
    fn decode(&mut self,
              input:&[u8],
              input_offset : &mut usize,
              output :&mut[u8],
              output_offset: &mut usize) -> DivansResult;
}

pub trait CommandDecoder {
    type CommandSliceType: SliceWrapper<u8>;
    fn decode(
        &mut self,
        input: &[u8],
        input_offset: &mut usize,
        output: &mut [Command<Self::CommandSliceType>],
        output_offset: &mut usize) -> DivansResult;
    fn flush(&mut self) -> DivansResult;
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub enum BillingDesignation {
    Unknown,
    CopyCommand(CopySubstate),
    DictCommand(DictSubstate),
    LiteralCommand(LiteralSubstate),
    CrossCommand(CrossCommandBilling),
    PredModeCtxMap(PredictionModeSubstate),
    BlockType(BlockTypeState),
}

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub enum CrossCommandBilling {
    Unknown,
    CopyIndicator,
    DictIndicator,
    EndIndicator,
    BlockSwitchType,
    FullSelection,
}

pub trait NewWithAllocator<AllocU8: Allocator<u8>> {
    fn new(m8: &mut AllocU8) -> Self;
    fn free(&mut self, m8: &mut AllocU8);
}

pub trait BillingCapability { // maybe we should have called it capa-bill-ity
    fn debug_print(&self, _size:usize) {
        //intentially a default noop, can be filled out by decoders
    }
}

pub trait EncoderOrDecoderRecoderSpecialization {
    fn get_recoder_output<'a>(&'a mut self, passed_in_output_bytes: &'a mut [u8]) -> &'a mut[u8];
    fn get_recoder_output_offset<'a>(&self,
                                     passed_in_output_bytes: &'a mut usize,
                                     backing: &'a mut usize) -> &'a mut usize;
    
}

pub trait ArithmeticEncoderOrDecoder : Sized {
    #[inline(always)]
    fn mov(&mut self) -> Self;
    #[inline(always)]
    fn mov_consume(self) -> Self {
        self
    }
    // note: only one of these buffers must be nonzero,
    // depending on if it is in encode or decode mode
    #[inline(always)]
    fn drain_or_fill_internal_buffer(&mut self,
                                     input:&mut ReadableBytes,
                                     output:&mut WritableBytes) -> DivansResult {
        if self.has_data_to_drain_or_fill() {
            self.drain_or_fill_internal_buffer_unchecked(input, output)
        } else {
            DivansResult::Success
        }
    }
    #[inline(always)]
    fn drain_or_fill_internal_buffer_unchecked(&mut self,
                                               input:&mut ReadableBytes,
                                               output:&mut WritableBytes) -> DivansResult;
    #[inline(always)]
    fn has_data_to_drain_or_fill(&self) -> bool;

    #[inline(always)]
    fn get_or_put_bit_without_billing(&mut self,
                                      bit: &mut bool,
                                      prob_of_false: u8);
    fn get_or_put_bit(&mut self,
                      bit: &mut bool,
                      prob_of_false: u8,
                      _billing: BillingDesignation) {
        self.get_or_put_bit_without_billing(bit, prob_of_false)
    }

    #[inline(always)]
    fn get_or_put_nibble_without_billing<C: CDF16>(&mut self,
                                                   nibble: &mut u8,
                                                   prob: &C) -> ProbRange;
    #[inline(always)]
    fn get_or_put_nibble<C: CDF16>(&mut self,
                                   nibble: &mut u8,
                                   prob: &C,
                                   _billing: BillingDesignation) -> ProbRange {
        self.get_or_put_nibble_without_billing(nibble, prob)
    }

    fn close(&mut self) -> DivansResult;
}
pub trait DivansCompressorFactory<
     AllocU8:Allocator<u8>,
     AllocU32:Allocator<u32>,
     AllocCDF16:Allocator<DefaultCDF16>> {
     type DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>;
     type ConstructedCompressor: Compressor;
     type AdditionalArgs;
    fn new(m8: AllocU8, m32: AllocU32, mcdf16:AllocCDF16,
           opts: DivansCompressorOptions,
           additional_args: Self::AdditionalArgs) -> Self::ConstructedCompressor;
}

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum BrotliCompressionSetting {
    UseInternalCommandSelection = 0,
    UseBrotliCommandSelection = 1,
    UseBrotliBitstream = 2,
}
impl Default for BrotliCompressionSetting {
    fn default() ->Self {
        BrotliCompressionSetting::UseBrotliCommandSelection
    }
}

#[derive(Default, Clone, Copy)]
pub struct DivansCompressorOptions{
    pub literal_adaptation: Option<[probability::Speed;4]>,
    pub window_size: Option<i32>,
    pub lgblock: Option<u32>,
    pub quality: Option<u16>,
    pub q9_5: bool,
    pub force_literal_context_mode: Option<LiteralPredictionModeNibble>,
    pub dynamic_context_mixing: Option<u8>,
    pub stride_detection_quality: Option<u8>,
    pub speed_detection_quality: Option<u8>,
    pub use_brotli: BrotliCompressionSetting,
    pub use_context_map: bool,
    pub force_stride_value: StrideSelection,
    pub prior_depth: Option<u8>,
    pub prior_bitmask_detection: u8,
    pub brotli_literal_byte_score: Option<u32>,
}
