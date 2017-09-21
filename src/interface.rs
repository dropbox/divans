use alloc::{SliceWrapper, Allocator};
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


#[derive(Debug)]
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

#[derive(Debug)]
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
    pub prob: SliceType,
}

impl<SliceType:SliceWrapper<u8>+Default> Nop<LiteralCommand<SliceType>> for LiteralCommand<SliceType> {
    fn nop() -> Self {
        LiteralCommand {
            data: SliceType::default()
        }
    }
}

#[derive(Debug)]
pub enum Command<SliceType:SliceWrapper<u8> > {
    Copy(CopyCommand),
    Dict(DictCommand),
    Literal(LiteralCommand<SliceType>),
    BlockSwitchCommand(BlockSwitch),
    BlockSwitchLiteral(BlockSwitch),
    BlockSwitchDistance(BlockSwitch),
    PredictionMode(PredictionModeContextMap<SliceType>),
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
    fn encode<SliceType:SliceWrapper<u8>+Default>(&mut self,
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
