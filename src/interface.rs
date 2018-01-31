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
use brotli::BrotliResult;
use super::probability::interface::{CDF2, CDF16, ProbRange};
use super::probability;
use super::codec::copy::CopySubstate;
use super::codec::dict::DictSubstate;
use super::codec::literal::LiteralSubstate;
use super::codec::context_map::PredictionModeState;
use super::codec::block_type::BlockTypeState;
pub use super::codec::StrideSelection;
pub use brotli::enc::interface::*;

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


pub trait Compressor {
    fn encode(&mut self,
              input:&[u8],
              input_offset: &mut usize,
              output:&mut[u8],
              output_offset:&mut usize) -> BrotliResult;
    fn encode_commands<SliceType:SliceWrapper<u8>+Default>(&mut self,
                                          input:&[Command<SliceType>],
                                          input_offset : &mut usize,
                                          output :&mut[u8],
                                          output_offset: &mut usize) -> BrotliResult;
    fn flush(&mut self,
                                          output :&mut[u8],
                                          output_offset: &mut usize) -> BrotliResult;
}

pub trait Decompressor {
    fn decode(&mut self,
              input:&[u8],
              input_offset : &mut usize,
              output :&mut[u8],
              output_offset: &mut usize) -> BrotliResult;
}

pub trait CommandDecoder {
    type CommandSliceType: SliceWrapper<u8>;
    fn decode(
        &mut self,
        input: &[u8],
        input_offset: &mut usize,
        output: &mut [Command<Self::CommandSliceType>],
        output_offset: &mut usize) -> BrotliResult;
    fn flush(&mut self) -> BrotliResult;
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub enum BillingDesignation {
    Unknown,
    CopyCommand(CopySubstate),
    DictCommand(DictSubstate),
    LiteralCommand(LiteralSubstate),
    CrossCommand(CrossCommandBilling),
    PredModeCtxMap(PredictionModeState),
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

pub trait ArithmeticEncoderOrDecoder {
    // note: only one of these buffers must be nonzero,
    // depending on if it is in encode or decode mode
    fn drain_or_fill_internal_buffer(&mut self,
                                     input_buffer:&[u8],
                                     input_offset:&mut usize,
                                     output_buffer:&mut [u8],
                                     output_offset: &mut usize) -> BrotliResult;
    fn get_or_put_bit_without_billing(&mut self,
                                      bit: &mut bool,
                                      prob_of_false: u8);
    fn get_or_put_bit(&mut self,
                      bit: &mut bool,
                      prob_of_false: u8,
                      _billing: BillingDesignation) {
        self.get_or_put_bit_without_billing(bit, prob_of_false)
    }

    fn get_or_put_nibble_without_billing<C: CDF16>(&mut self,
                                                   nibble: &mut u8,
                                                   prob: &C) -> ProbRange;
    fn get_or_put_nibble<C: CDF16>(&mut self,
                                   nibble: &mut u8,
                                   prob: &C,
                                   _billing: BillingDesignation) -> ProbRange {
        self.get_or_put_nibble_without_billing(nibble, prob)
    }

    fn close(&mut self) -> BrotliResult;
}
pub trait DivansCompressorFactory<
     AllocU8:Allocator<u8>,
     AllocU32:Allocator<u32>,
     AllocCDF2:Allocator<CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> {
     type DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>;
     type ConstructedCompressor: Compressor;
     type AdditionalArgs;
    fn new(m8: AllocU8, m32: AllocU32, mcdf2:AllocCDF2, mcdf16:AllocCDF16, window_size: usize,
           dynamic_context_mixing: u8,
           prior_depth: Option<u8>,
           literal_adaptation_rate: Option<[probability::Speed;4]>,
           do_context_map: bool,
           force_stride: StrideSelection,
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
    pub dynamic_context_mixing: Option<u8>,
    pub stride_detection_quality: Option<u8>,
    pub use_brotli: BrotliCompressionSetting,
    pub use_context_map: bool,
    pub force_stride_value: StrideSelection,
    pub prior_depth: Option<u8>,
}
