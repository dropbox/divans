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
use core;
use brotli_decompressor::BrotliResult;
use super::probability::CDF16;
use super::codec::{CopySubstate, DictSubstate, LiteralSubstate, PredictionModeState};

// Commands that can instantiate as a no-op should implement this.
pub trait Nop<T> {
    fn nop() -> T;
}

#[derive(Debug,Copy,Clone,Default)]
pub struct BlockSwitch(u8);

impl BlockSwitch {
    pub fn new(block_type: u8) -> Self {
        BlockSwitch(block_type)
    }
    pub fn block_type(&self) -> u8 {
        self.0
    }
}

#[derive(Debug,Copy,Clone,Default)]
pub struct LiteralBlockSwitch(pub BlockSwitch, u8);

impl LiteralBlockSwitch {
    pub fn new(block_type: u8, stride: u8) -> Self {
        LiteralBlockSwitch(BlockSwitch::new(block_type), stride)
    }
    pub fn block_type(&self) -> u8 {
        self.0.block_type()
    }
    pub fn stride(&self) -> u8 {
        self.1
    }
}

pub const LITERAL_PREDICTION_MODE_SIGN: u8 = 3;
pub const LITERAL_PREDICTION_MODE_UTF8: u8 = 2;
pub const LITERAL_PREDICTION_MODE_MSB6: u8 = 1;
pub const LITERAL_PREDICTION_MODE_LSB6: u8 = 0;

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LiteralPredictionModeNibble(pub u8);

impl LiteralPredictionModeNibble {
    pub fn new(prediction_mode: u8) -> Result<Self, ()> {
        if prediction_mode < 16 {
            return Ok(LiteralPredictionModeNibble(prediction_mode));
        }
        return Err(());
    }
    pub fn prediction_mode(&self) -> u8 {
        self.0
    }
    pub fn signed() -> Self {
        LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_SIGN)
    }
    pub fn utf8() -> Self {
        LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_UTF8)
    }
    pub fn msb6() -> Self {
        LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_MSB6)
    }
    pub fn lsb6() -> Self {
        LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_LSB6)
    }
}
#[derive(Debug)]
pub struct PredictionModeContextMap<SliceType:SliceWrapper<u8>> {
    pub literal_prediction_mode: LiteralPredictionModeNibble,
    pub literal_context_map: SliceType,
    pub distance_context_map: SliceType,
}

impl<SliceType:SliceWrapper<u8>+Default+Clone> Clone for PredictionModeContextMap<SliceType> {
    fn clone(&self) -> Self {
        PredictionModeContextMap {
            literal_prediction_mode:self.literal_prediction_mode,
            literal_context_map: self.literal_context_map.clone(),
            distance_context_map: self.distance_context_map.clone(),
        }
    }
}
impl<SliceType:SliceWrapper<u8>+Default+Clone+Copy> Copy for PredictionModeContextMap<SliceType> {
}



#[derive(Debug, Clone, Copy)]
pub struct CopyCommand {
    pub distance: u32,
    pub num_bytes: u32,
}

impl Nop<CopyCommand> for CopyCommand {
    fn nop() -> Self {
        CopyCommand {
            distance: 1,
            num_bytes: 0
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DictCommand {
    pub word_size: u8,
    pub transform: u8,
    pub final_size: u8,
    pub empty: u8,
    pub word_id: u32,
}

impl Nop<DictCommand> for DictCommand {
    fn nop() -> Self {
        DictCommand {
            word_size: 0,
            transform: 0,
            final_size: 0,
            empty: 1,
            word_id: 0
        }
    }
}

#[derive(Debug)]
pub struct LiteralCommand<SliceType:SliceWrapper<u8>> {
    pub data: SliceType,
}

impl<SliceType:SliceWrapper<u8>+Default> Nop<LiteralCommand<SliceType>> for LiteralCommand<SliceType> {
    fn nop() -> Self {
        LiteralCommand {
            data: SliceType::default()
        }
    }
}

impl<SliceType:SliceWrapper<u8>+Default+Clone> Clone for LiteralCommand<SliceType> {
    fn clone(&self) -> Self {
        LiteralCommand {
            data: self.data.clone()
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

/*
impl<SliceType:SliceWrapper<u8>+Default> Command<SliceType> {
    pub fn free_array<F>(&mut self, apply_func: &mut F) where F: FnMut(SliceType) {
       match self {
          &mut Command::Literal(ref mut lit) => {
             apply_func(core::mem::replace(&mut lit.data, SliceType::default()))
          },
          &mut Command::PredictionMode(ref mut pm) => {
             apply_func(core::mem::replace(&mut pm.literal_context_map, SliceType::default()));
             apply_func(core::mem::replace(&mut pm.distance_context_map, SliceType::default()));
          },
          _ => {},
       }
    }
}
*/
pub fn free_cmd<SliceTypeAllocator:Allocator<u8>> (xself: &mut Command<SliceTypeAllocator::AllocatedMemory>, m8: &mut SliceTypeAllocator) {
       match xself {
          &mut Command::Literal(ref mut lit) => {
             m8.free_cell(core::mem::replace(&mut lit.data, SliceTypeAllocator::AllocatedMemory::default()))
          },
          &mut Command::PredictionMode(ref mut pm) => {
             m8.free_cell(core::mem::replace(&mut pm.literal_context_map, SliceTypeAllocator::AllocatedMemory::default()));
             m8.free_cell(core::mem::replace(&mut pm.distance_context_map, SliceTypeAllocator::AllocatedMemory::default()));
          },
          _ => {},
    }
}
impl<SliceType:SliceWrapper<u8>> Default for Command<SliceType> {
    fn default() -> Self {
        Command::<SliceType>::nop()
    }
}

impl<SliceType:SliceWrapper<u8>> Nop<Command<SliceType>> for Command<SliceType> {
    fn nop() -> Command<SliceType> {
        Command::Copy(CopyCommand::nop())
    }
}

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
                                                   prob: &C);
    fn get_or_put_nibble<C: CDF16>(&mut self,
                                   nibble: &mut u8,
                                   prob: &C,
                                   _billing: BillingDesignation) {
        self.get_or_put_nibble_without_billing(nibble, prob)
    }

    fn close(&mut self) -> BrotliResult;
}
