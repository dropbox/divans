use core;
use interface::{DivansOpResult, ErrMsg, StreamMuxer, StreamDemuxer, DivansResult, WritableBytes};
use ::cmd_to_raw::DivansRecodeState;
use ::probability::{CDF16, Speed};
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use ::slice_util::AllocatedMemoryPrefix;
use ::alloc_util::RepurposingAlloc;
use ::constants;
use ::interface::{
    ArithmeticEncoderOrDecoder,
    CrossCommandBilling,
    Command,
    CopyCommand,
    PredictionModeContextMap,
    DictCommand,
    LiteralCommand,
    LiteralBlockSwitch,
    LiteralPredictionModeNibble,
    LITERAL_PREDICTION_MODE_SIGN,
    LITERAL_PREDICTION_MODE_UTF8,
    LITERAL_PREDICTION_MODE_MSB6,
    LITERAL_PREDICTION_MODE_LSB6,
    MAX_ADV_LITERAL_CONTEXT_MAP_SIZE,
    NewWithAllocator,
    EncoderOrDecoderRecoderSpecialization,
};
use super::priors::{
    LiteralNibblePriors,
    LiteralCommandPriors,
    LiteralCommandPriorsCM,
    CopyCommandPriors,
    DictCommandPriors,
    CrossCommandPriors,
    PredictionModePriors,
    BlockTypePriors,
    NUM_BLOCK_TYPES,
};
use ::priors::PriorCollection;
const LOG_NUM_COPY_TYPE_PRIORS: usize = 4;

pub const BLOCK_TYPE_LITERAL_SWITCH:usize=0;
pub const BLOCK_TYPE_COMMAND_SWITCH:usize=1;
pub const BLOCK_TYPE_DISTANCE_SWITCH:usize=2;
pub const NUM_ARITHMETIC_CODERS:usize = 2;
pub const CMD_CODER: usize = 0;
pub const LIT_CODER: usize = 1;
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum StrideSelection {
    PriorDisabled = 0u8,
    Stride1 = 1u8,
    Stride2 = 2u8,
    Stride3 = 3u8,
    Stride4 = 4u8,
    Stride5 = 5u8,
    Stride6 = 6u8,
    Stride7 = 7u8,
    Stride8 = 8u8,
    UseBrotliRec = 9u8,
}

impl Default for StrideSelection {
    fn default() -> Self {
        StrideSelection::UseBrotliRec
    }
}

pub trait EncoderOrDecoderSpecialization {
    const DOES_CALLER_WANT_ORIGINAL_FILE_BYTES: bool;
    const IS_DECODING_FILE: bool;
    fn alloc_literal_buffer<AllocU8: Allocator<u8>>(&mut self,
                                                    m8: &mut AllocU8,
                                                    len: usize) -> AllocatedMemoryPrefix<u8, AllocU8>;
    fn get_input_command<'a, ISlice:SliceWrapper<u8>>(&self, data:&'a [Command<ISlice>],offset: usize,
                                                      backing:&'a Command<ISlice>) -> &'a Command<ISlice>;
    fn get_output_command<'a, AllocU8:Allocator<u8>>(&self, data:&'a mut [Command<AllocatedMemoryPrefix<u8, AllocU8>>],
                                                     offset: usize,
                                                     backing:&'a mut Command<AllocatedMemoryPrefix<u8, AllocU8>>) -> &'a mut Command<AllocatedMemoryPrefix<u8, AllocU8>>;
    fn get_source_copy_command<'a, ISlice:SliceWrapper<u8>>(&self, &'a Command<ISlice>, &'a CopyCommand) -> &'a CopyCommand;
    fn get_source_literal_command<'a, ISlice:SliceWrapper<u8>+Default>(&self, &'a Command<ISlice>,
                                                                       &'a LiteralCommand<ISlice>) -> &'a LiteralCommand<ISlice>;
    fn get_source_dict_command<'a, ISlice:SliceWrapper<u8>>(&self, &'a Command<ISlice>, &'a DictCommand) -> &'a DictCommand;
    fn get_literal_byte<ISlice:SliceWrapper<u8>>(&self,
                                                   in_cmd: &LiteralCommand<ISlice>,
                                                   index: usize) -> u8;
    fn get_recoder_output<'a>(&'a mut self, passed_in_output_bytes: &'a mut [u8]) -> &'a mut[u8];
    fn get_recoder_output_offset<'a>(&self,
                                     passed_in_output_bytes: &'a mut usize,
                                     backing: &'a mut usize) -> &'a mut usize;
}

impl <T:EncoderOrDecoderSpecialization> EncoderOrDecoderRecoderSpecialization for T {
    fn get_recoder_output<'a>(&'a mut self, passed_in_output_bytes: &'a mut [u8]) -> &'a mut[u8] {
        <Self as EncoderOrDecoderSpecialization>::get_recoder_output(self, passed_in_output_bytes)
    }
    fn get_recoder_output_offset<'a>(&self,
                                     passed_in_output_bytes: &'a mut usize,
                                     backing: &'a mut usize) -> &'a mut usize {
        <Self as EncoderOrDecoderSpecialization>::get_recoder_output_offset(self,passed_in_output_bytes, backing)
    }
}






#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ContextMapType {
    Literal,
    Distance
}


#[derive(Copy,Clone)]
pub struct DistanceCacheEntry {
    pub distance:u32,
    pub command_count:u64,
}

const CONTEXT_MAP_CACHE_SIZE: usize = 13;

pub struct LiteralBookKeeping<Cdf16:CDF16,
                                   AllocU8:Allocator<u8>,
                                   AllocCDF16:Allocator<Cdf16>> {
    pub last_8_literals: u64,
    pub literal_context_map: AllocU8::AllocatedMemory,
    pub btype_last: u8,
    pub stride: u8,
    pub combine_literal_predictions: bool,
    pub literal_prediction_mode: LiteralPredictionModeNibble,
    pub literal_adaptation: [Speed; 4],
    pub literal_lut0:[u8;256],
    pub literal_lut1:[u8;256],
    pub mixing_mask:[u8; 8192],
    pub model_weights: [super::weights::Weights;2],
    pub materialized_context_map: bool,
    pub lit_cm_priors: LiteralCommandPriorsCM<Cdf16, AllocCDF16>,
}

pub struct CrossCommandBookKeeping<Cdf16:CDF16,
                                   AllocU8:Allocator<u8>,
                                   AllocCDF16:Allocator<Cdf16>> {
    //pub command_count:u32,
    //pub num_literals_coded: u32,
    pub lit_len_priors: LiteralCommandPriors<Cdf16, AllocCDF16>,
    pub distance_context_map: AllocU8::AllocatedMemory,
    pub cc_priors: CrossCommandPriors<Cdf16, AllocCDF16>,
    pub copy_priors: CopyCommandPriors<Cdf16, AllocCDF16>,
    pub dict_priors: DictCommandPriors<Cdf16, AllocCDF16>,
    pub prediction_priors: PredictionModePriors<Cdf16, AllocCDF16>,
    pub cmap_lru: [u8; CONTEXT_MAP_CACHE_SIZE],
    pub distance_lru: [u32;4],
    pub btype_priors: BlockTypePriors<Cdf16, AllocCDF16>,
    pub distance_cache:[[DistanceCacheEntry;3];32],
    pub btype_lru: [[u8;2];3],
    pub btype_max_seen: [u8;3],
    //pub cm_prior_depth_mask: u8,
    //pub prior_bytes_depth_mask: u8,
    pub last_dlen: u8,
    pub last_clen: u8,
    pub last_llen: u32,
    pub last_4_states: u8,
    pub materialized_context_map: bool,
    pub desired_prior_depth: u8,
    pub desired_literal_adaptation: Option<[Speed;4]>,
    pub desired_do_context_map: bool,
    pub desired_force_stride: StrideSelection,
    pub desired_context_mixing: u8,
    pub command_count: u64,
}

#[inline(always)]
fn sub_or_add(val: u32, sub: u32, add: u32) -> u32 {
    core::cmp::min(val.wrapping_sub(sub), val.wrapping_add(add))
/*    if val >= sub {
        val - sub
    } else {
        val + add
    }*/
}
#[inline(always)]
pub fn round_up_mod_4(val: u8) -> u8 {
    ((val - 1)|3)+1
}
#[inline(always)]
pub fn round_up_mod_4_u32(val: u32) -> u32 {
    ((val - 1)|3)+1
}
#[inline(always)]
pub fn default_literal_speed() -> Speed {
    Speed::MUD
}

#[derive(Clone,Copy,Debug)]
pub struct ByteContext {
  pub stride_bytes: u64,
  pub actual_context: [u8;2],
  pub prev_byte: u8,
}

fn get_lut0(lpn: LiteralPredictionModeNibble) -> [u8; 256] {
    let mut ret = [0u8; 256];
    match lpn.0 {
        LITERAL_PREDICTION_MODE_SIGN =>
            for (i, j) in ret.iter_mut().zip(constants::SIGNED_3_BIT_CONTEXT_LOOKUP.iter()) {
                *i = *j << 3;
            },
        LITERAL_PREDICTION_MODE_UTF8 =>
            for (i, j) in ret.iter_mut().zip(constants::UTF8_CONTEXT_LOOKUP.split_at(256).0.iter()) {
                *i = *j;
            },
        LITERAL_PREDICTION_MODE_MSB6 =>
            for (index, val) in ret.iter_mut().enumerate() {
                *val = (index as u8) >> 2;
            },
        LITERAL_PREDICTION_MODE_LSB6 =>
            for (index, val) in ret.iter_mut().enumerate() {
                *val = (index as u8) & 0x3f;
            },
        _ => panic!("Internal Error: parsed nibble prediction mode has more than 2 bits"),
    }
    ret
}
fn get_lut1(lpn: LiteralPredictionModeNibble) -> [u8; 256] {
    let mut ret = [0u8; 256];
    match lpn.0 {
        LITERAL_PREDICTION_MODE_SIGN =>
            for (i, j) in ret.iter_mut().zip(constants::SIGNED_3_BIT_CONTEXT_LOOKUP.iter()) {
                *i = *j;
            },
        LITERAL_PREDICTION_MODE_UTF8 =>
            for (i, j) in ret.iter_mut().zip(constants::UTF8_CONTEXT_LOOKUP.split_at(256).1.iter()) {
                *i = *j;
            },
        LITERAL_PREDICTION_MODE_MSB6 => {}, // empty
        LITERAL_PREDICTION_MODE_LSB6 => {}, // empty
        _ => panic!("Internal Error: parsed nibble prediction mode has more than 2 bits"),
    }
    ret
}

impl<                                   
     Cdf16:CDF16,
     AllocCDF16:Allocator<Cdf16>,
     AllocU8:Allocator<u8>> LiteralBookKeeping<Cdf16,
                                               AllocU8,
                                               AllocCDF16> {
    fn new(literal_context_map:AllocU8::AllocatedMemory) -> Self {
        LiteralBookKeeping::<Cdf16, AllocU8, AllocCDF16> {
            //materialized_context_map: false,
            combine_literal_predictions: false,
            last_8_literals: 0,
            stride: 0,
            literal_adaptation: [default_literal_speed(); 4],
            literal_prediction_mode: LiteralPredictionModeNibble::default(),
            literal_lut0: get_lut0(LiteralPredictionModeNibble::default()),
            literal_lut1: get_lut1(LiteralPredictionModeNibble::default()),
            mixing_mask: [0;8192],
            literal_context_map:literal_context_map,
            btype_last:0,
            model_weights:[super::weights::Weights::default(),
                           super::weights::Weights::default()],
            materialized_context_map: false,
            lit_cm_priors: LiteralCommandPriorsCM {
                priors: AllocCDF16::AllocatedMemory::default()
            },
        }
    }
    pub fn get_literal_block_type(&self) -> u8 {
        self.btype_last
    }
    pub fn obs_pred_mode(&mut self, new_mode: LiteralPredictionModeNibble) -> DivansOpResult {
       // self.next_state(); // FIXME removing: but it seems wrong
       match new_mode.0 {
           LITERAL_PREDICTION_MODE_SIGN | LITERAL_PREDICTION_MODE_UTF8 | LITERAL_PREDICTION_MODE_MSB6 | LITERAL_PREDICTION_MODE_LSB6 => {
           },
           _ => return DivansOpResult::Failure(ErrMsg::PredictionModeOutOfBounds(new_mode.0)),
       }
       self.literal_prediction_mode = new_mode;
       self.literal_lut0 = get_lut0(new_mode);
       self.literal_lut1 = get_lut1(new_mode);
       DivansOpResult::Success
    }
    pub fn push_literal_byte(&mut self, b: u8) {
        //self.num_literals_coded += 1;
        self.last_8_literals >>= 0x8;
        self.last_8_literals |= u64::from(b) << 0x38;
    }
    pub fn push_literal_nibble(&mut self, nibble: u8) {
        self.last_8_literals >>= 0x4;
        self.last_8_literals |= u64::from(nibble) << 0x3c;
    }
    pub fn obs_literal_block_switch(&mut self, btype:LiteralBlockSwitch) {
        self.btype_last = btype.block_type();
        self.stride = btype.stride();
    }
    pub fn obs_prediction_mode_context_map<ISlice:SliceWrapper<u8>>(&mut self,
                                                                    pm: &PredictionModeContextMap<ISlice>,
                                                                    mcdf16: &mut AllocCDF16) -> DivansOpResult {
        self.reset_literal_context_map();
        let mut combined_prediction_mode = pm.literal_prediction_mode();
        let context_mixing = combined_prediction_mode.0 >> 6;
        self.obs_dynamic_context_mixing(if context_mixing != 0 {context_mixing -1} else {0}, mcdf16);
        combined_prediction_mode.0 &= 0xf;
        match self.obs_pred_mode(combined_prediction_mode) {
            DivansOpResult::Success => {},
            fail => return fail,
        }
        for (out_item, in_item) in self.literal_adaptation[..2].iter_mut().zip(pm.stride_context_speed_f8().iter()) {
            *out_item = Speed::from_f8_tuple(*in_item);
        }
        for (out_item, in_item) in self.literal_adaptation[2..].iter_mut().zip(pm.context_map_speed_f8().iter()) {
            *out_item = Speed::from_f8_tuple(*in_item);
        }
        self.literal_context_map.slice_mut().clone_from_slice(pm.literal_context_map.slice());
        // self.distance_context_map.slice_mut().clone_from_slice(pm.distance_context_map()); // FIXME: this was done during parsing of the pm
        for item in self.literal_context_map.slice().iter() {
            if *item != 0 {
                self.materialized_context_map = true;
                break;
            }
        }
        if pm.get_mixing_values().len() != self.mixing_mask.len() {
            self.clear_mixing_values();
        }
        self.mixing_mask.clone_from_slice(pm.get_mixing_values());
        DivansOpResult::Success
    }
    pub fn obs_dynamic_context_mixing(&mut self, context_mixing: u8, mcdf16: &mut AllocCDF16) {
        self.combine_literal_predictions = (context_mixing != 0) as bool;
        if context_mixing >= 2 && self.lit_cm_priors.priors.slice().len() == 0 {
            self.lit_cm_priors.priors = mcdf16.alloc_cell(LiteralCommandPriorsCM::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        }
        self.model_weights[0].set_mixing_param(context_mixing);
        self.model_weights[1].set_mixing_param(context_mixing);
    }
    pub fn clear_mixing_values(&mut self) {
        for item in self.mixing_mask.iter_mut()  {
            *item = 0;
        }
    }
    pub fn reset_literal_context_map(&mut self) {
        for (index, item) in self.literal_context_map.slice_mut().iter_mut().enumerate() {
            *item = index as u8 & 0x3f;
        }
    }
}
impl<                                   
     Cdf16:CDF16,
     AllocCDF16:Allocator<Cdf16>,
     AllocU8:Allocator<u8>> CrossCommandBookKeeping<
                                                    Cdf16,
                                                    AllocU8,
                                                    AllocCDF16> {
    fn new(lit_len_prior:AllocCDF16::AllocatedMemory,
           cc_prior: AllocCDF16::AllocatedMemory,
           copy_prior: AllocCDF16::AllocatedMemory,
           dict_prior: AllocCDF16::AllocatedMemory,
           pred_prior: AllocCDF16::AllocatedMemory,
           btype_prior: AllocCDF16::AllocatedMemory,
           distance_context_map: AllocU8::AllocatedMemory,
           mut dynamic_context_mixing: u8,
           prior_depth: u8,
           literal_adaptation_speed:Option<[Speed;4]>,
           do_context_map: bool,
           force_stride: StrideSelection) -> Self {
        assert!(dynamic_context_mixing < 15); // leaves room for expansion
        match force_stride {
            StrideSelection::PriorDisabled => {},
            _ => if dynamic_context_mixing == 0 && do_context_map {
                dynamic_context_mixing = 1; // make sure to mix if user requested both context map and stride
            },
        }
        CrossCommandBookKeeping{
            desired_prior_depth:prior_depth,
            desired_literal_adaptation: literal_adaptation_speed,
            desired_context_mixing:dynamic_context_mixing,
            command_count: 0,
            distance_cache:[
                [
                    DistanceCacheEntry{
                        distance:1,
                        command_count:0,
                    };3];32],
            last_dlen: 1,
            last_llen: 1,
            last_clen: 1,
            materialized_context_map: false,
            // FIXME combine_literal_predictions: false,
            last_4_states: 3 << (8 - LOG_NUM_COPY_TYPE_PRIORS),
            cmap_lru: [0u8; CONTEXT_MAP_CACHE_SIZE],
            lit_len_priors: LiteralCommandPriors {
                priors: lit_len_prior,
            },
            prediction_priors: PredictionModePriors {
                priors: pred_prior,
            },
            cc_priors: CrossCommandPriors::<Cdf16, AllocCDF16> {
                priors: cc_prior
            },
            copy_priors: CopyCommandPriors {
                priors: copy_prior
            },
            dict_priors: DictCommandPriors {
                priors: dict_prior
            },
            distance_context_map:distance_context_map,
            btype_priors: BlockTypePriors {
                priors: btype_prior
            },
            distance_lru: [4,11,15,16],
            btype_lru:[[0,1];3],
            btype_max_seen:[0;3],
            desired_do_context_map: do_context_map,
            desired_force_stride:force_stride,
        }
    }
    pub fn materialized_prediction_mode(&self) -> bool {
        self.materialized_context_map
    }
    /* DEPRECATED
    pub fn obs_mixing_value(&mut self, index: usize, value: u8) -> DivansOpResult {
        //if index >= self.mixing_mask.len() {
        //return DivansResult::Failure;
        //}
        self.mixing_mask[index] = value;
        DivansOpResult::Success
}*/
    /* DEPRECATED
    pub fn obs_literal_adaptation_rate(&mut self, index: u32, ladaptation_rate: Speed) {
        if index < self.literal_adaptation.len() as u32 {
            self.literal_adaptation[index as usize
            ] = ladaptation_rate;
        }
    }*/
    pub fn obs_prior_depth(&mut self, _prior_depth: u8) {
        /*
        self.cm_prior_depth_mask = ((1u32 << core::cmp::min(prior_depth, 8)) - 1) as u8;
        self.prior_bytes_depth_mask = ((1u32 << (7 - core::cmp::min(prior_depth, 8))) - 1) as u8;
        self.prior_bytes_depth_mask = !self.prior_bytes_depth_mask; //bitwise not to grab upper bit
         */
    }

    pub fn get_distance_prior(&mut self, copy_len: u32) -> usize {
        let dtype = self.get_distance_block_type() as usize;
        let distance_map_index = dtype * 4 + core::cmp::min(core::cmp::max(copy_len, 2) - 2, 3) as usize;
        self.distance_context_map.slice()[distance_map_index] as usize
    }
    pub fn reset_context_map_lru(&mut self) {
        self.cmap_lru = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    }
    pub fn reset_distance_context_map(&mut self) {
        for (index, item) in self.distance_context_map.slice_mut().iter_mut().enumerate() {
            *item = index as u8 & 0x3;
        }
    }
    pub fn obs_context_map_for_lru(&mut self, context_map_type: ContextMapType, index : u32, val: u8) -> DivansOpResult {
        if val != 0 {
            self.materialized_context_map = true;
        }
        match self.cmap_lru.iter().enumerate().find(|x| *x.1 == val) {
            Some((index, _)) => {
                if index != 0 {
                    let tmp = self.cmap_lru; // clone
                    self.cmap_lru[1..index + 1].clone_from_slice(&tmp[..index]);
                    self.cmap_lru[index + 1..].clone_from_slice(&tmp[(index + 1)..]);
                }
            },
            None => {
                let tmp = self.cmap_lru; // clone
                self.cmap_lru[1..].clone_from_slice(&tmp[..(tmp.len() - 1)]);
            },
        }
        self.cmap_lru[0] = val;
        match context_map_type {
            ContextMapType::Literal => {
            }
            ContextMapType::Distance => {
                if (index as usize) < self.distance_context_map.slice().len() {
                    self.distance_context_map.slice_mut()[index as usize] = val;
                } else {
                    return DivansOpResult::Failure(ErrMsg::IndexBeyondContextMapSize(index as u8 & 0xff,
                                                                                     (index >> 8) as u8 & 0xff));
                }
            }
        }
        DivansOpResult::Success
    }
    #[inline(always)]
    pub fn get_distance_from_mnemonic_code(&self, code:u8) -> (u32, bool) {
        /*match code & 0xf { // old version: measured to make the entire decode process take 112% as long
            0 => self.distance_lru[0],
            1 => self.distance_lru[1],
            2 => self.distance_lru[2],
            3 => self.distance_lru[3],
            4 => self.distance_lru[0] + 1,
            5 => self.distance_lru[0].wrapping_sub(1),
            6 => self.distance_lru[1] + 1,
            7 => self.distance_lru[1].wrapping_sub(1),
            8 => self.distance_lru[0] + 2,
            9 => self.distance_lru[0].wrapping_sub(2),
            10 => self.distance_lru[1] + 2,
            11 => self.distance_lru[1].wrapping_sub(2),
            12 => self.distance_lru[0] + 3,
            13 => self.distance_lru[0].wrapping_sub(3),
            14 => self.distance_lru[1] + 3,
            15 => self.distance_lru[0], // logic error
            _ => panic!("Logic error: nibble > 14 evaluated for nmemonic"),
        }*/
        if code < 4 {
            return (self.distance_lru[code as usize], true); // less than four is a fetch
        }
        let unsigned_summand = (code >> 2) as i32; // greater than four either adds or subtracts
        // the value depending on if its an even or odd code
        // mnemonic 1 are codes that have bit 2 set, mnemonic 0 are codes that don't have bit 2 set
        let signed_summand = unsigned_summand - (((-(code as i32 & 1)) & unsigned_summand) << 1);
        let ret = (self.distance_lru[((code & 2) >> 1) as usize] as i32) + signed_summand;
        (ret as u32, ret > 0)
    }
    pub fn distance_mnemonic_code(&self, d: u32) -> u8 {
        for i in 0..15 {
            let (item, ok) = self.get_distance_from_mnemonic_code(i as u8);
            if item == d && ok {
                return i as u8;
            }
        }
        15
    }
    pub fn get_command_block_type(&self) -> usize {
        self.btype_lru[BLOCK_TYPE_COMMAND_SWITCH][0] as usize
    }
    pub fn get_distance_block_type(&self) -> usize {
        self.btype_lru[BLOCK_TYPE_DISTANCE_SWITCH][0] as usize
    }
    pub fn get_literal_block_type(&self) -> usize {
        self.btype_lru[BLOCK_TYPE_LITERAL_SWITCH][0] as usize
    }
    pub fn get_command_type_prob(&mut self) -> &mut Cdf16 {
        //let last_8 = self.cross_command_state.recoder.last_8_literals();
        self.cc_priors.get(CrossCommandBilling::FullSelection,
                           ((self.last_4_states as usize) >> (8 - LOG_NUM_COPY_TYPE_PRIORS),
                           0)) // FIXME <-- improve this prior now that we are missing literals
    }
    fn next_state(&mut self) {
        self.last_4_states >>= 2;
    }

    pub fn obs_dict_state(&mut self) {
        self.next_state();
        self.last_4_states |= 192;
    }
    pub fn obs_copy_state(&mut self) {
        self.next_state();
        self.last_4_states |= 64;
    }
    pub fn obs_literal_state(&mut self) {
        self.next_state();
        self.last_4_states |= 128;
    }
    pub fn obs_distance(&mut self, cc:&CopyCommand) {
        if cc.num_bytes < self.distance_cache.len() as u32{
            let nb = cc.num_bytes as usize;
            let mut sub_index = if self.distance_cache[nb][1].command_count < self.distance_cache[nb][0].command_count {
                1
            } else {
                0
            };
            if self.distance_cache[nb][2].command_count < self.distance_cache[nb][sub_index].command_count {
                sub_index = 2;
            }
            self.distance_cache[nb][sub_index] = DistanceCacheEntry{
                distance: 0,//cc.distance, we're copying it to here (ha!)
                command_count:self.command_count,
            };
        }
        let distance = cc.distance;
        if distance == self.distance_lru[1] {
            self.distance_lru = [distance,
                                 self.distance_lru[0],
                                 self.distance_lru[2],
                                 self.distance_lru[3]];
        } else if distance == self.distance_lru[2] {
            self.distance_lru = [distance,
                                 self.distance_lru[0],
                                 self.distance_lru[1],
                                 self.distance_lru[3]];
        } else if distance != self.distance_lru[0] {
            self.distance_lru = [distance,
                                 self.distance_lru[0],
                                 self.distance_lru[1],
                                 self.distance_lru[2]];
        }
    }
    fn _obs_btype_helper(&mut self, btype_type: usize, btype: u8) {
        self.next_state();
        self.btype_lru[btype_type] = [btype, self.btype_lru[btype_type][0]];
        self.btype_max_seen[btype_type] = core::cmp::max(self.btype_max_seen[btype_type], btype);
    }
    pub fn obs_btypel(&mut self, btype:LiteralBlockSwitch) {
        self._obs_btype_helper(BLOCK_TYPE_LITERAL_SWITCH, btype.block_type());
    }
    pub fn obs_btypec(&mut self, btype:u8) {
        self._obs_btype_helper(BLOCK_TYPE_COMMAND_SWITCH, btype);
    }
    pub fn obs_btyped(&mut self, btype:u8) {
        self._obs_btype_helper(BLOCK_TYPE_DISTANCE_SWITCH, btype);
    }
}

pub struct MainThreadContext<Cdf16:CDF16, AllocU8:Allocator<u8>, AllocCDF16:Allocator<Cdf16>, ArithmeticCoder:ArithmeticEncoderOrDecoder> {
    pub recoder: DivansRecodeState<AllocU8::AllocatedMemory>,
    pub m8: RepurposingAlloc<u8, AllocU8>,
    pub mcdf16: AllocCDF16,
    pub lbk: LiteralBookKeeping<Cdf16, AllocU8, AllocCDF16>,
    pub lit_high_priors: LiteralNibblePriors<Cdf16, AllocCDF16>,
    pub lit_low_priors: LiteralNibblePriors<Cdf16, AllocCDF16>,
    pub lit_coder: ArithmeticCoder,
}

pub enum ThreadContext<Cdf16:CDF16, AllocU8:Allocator<u8>, AllocCDF16:Allocator<Cdf16>, ArithmeticCoder:ArithmeticEncoderOrDecoder> {
    MainThread(MainThreadContext<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder>),
    Worker,
}
impl <Cdf16:CDF16, AllocU8:Allocator<u8>, AllocCDF16:Allocator<Cdf16>, ArithmeticCoder:ArithmeticEncoderOrDecoder> MainThreadContext<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder> {
    pub fn free(&mut self) {
        self.m8.free_cell(core::mem::replace(&mut self.recoder.ring_buffer, AllocU8::AllocatedMemory::default()));
        self.m8.free_cell(core::mem::replace(&mut self.lbk.literal_context_map, AllocU8::AllocatedMemory::default()));
        self.mcdf16.free_cell(core::mem::replace(&mut self.lit_high_priors.priors, AllocCDF16::AllocatedMemory::default()));
        self.mcdf16.free_cell(core::mem::replace(&mut self.lit_low_priors.priors, AllocCDF16::AllocatedMemory::default()));
        self.mcdf16.free_cell(core::mem::replace(&mut self.lbk.lit_cm_priors.priors, AllocCDF16::AllocatedMemory::default()));
    }
}
impl <Cdf16:CDF16, AllocU8:Allocator<u8>, AllocCDF16:Allocator<Cdf16>, ArithmeticCoder:ArithmeticEncoderOrDecoder> ThreadContext<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder> {
    pub fn free(&mut self) {
        match *self {
            ThreadContext::MainThread(ref mut ctx) => ctx.free(),
            ThreadContext::Worker => {},
        }
    }
    pub fn lit_coder(&mut self) -> Option<&mut ArithmeticCoder> {
        match *self {
            ThreadContext::MainThread(ref mut ctx) => Some(&mut ctx.lit_coder),
            ThreadContext::Worker => None,
        }        
    }
    pub fn main_thread_mut(&mut self) -> Option<&mut MainThreadContext<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder>> {
        match *self {
            ThreadContext::MainThread(ref mut ctx) => Some(ctx),
            ThreadContext::Worker => None,
        }
    }
    pub fn m8(&mut self) ->Option<&mut RepurposingAlloc<u8, AllocU8>> {
        match *self {
            ThreadContext::MainThread(ref mut ctx) => Some(&mut ctx.m8),
            ThreadContext::Worker => None,
        }
    }
    pub fn recoder(&mut self) -> Option<&mut DivansRecodeState<AllocU8::AllocatedMemory>> {
        match *self {
            ThreadContext::MainThread(ref mut ctx) => Some(&mut ctx.recoder),
            ThreadContext::Worker => None,
        }
    }
    pub fn mcdf16(&mut self) -> Option<&mut AllocCDF16> {
        match *self {
            ThreadContext::MainThread(ref mut ctx) => Some(&mut ctx.mcdf16),
            ThreadContext::Worker => None,
        }
    }
    pub fn m8lbk(&mut self) ->(Option<&mut RepurposingAlloc<u8, AllocU8>>, Option<&mut LiteralBookKeeping<Cdf16, AllocU8, AllocCDF16>>) {
        match *self {
            ThreadContext::MainThread(ref mut ctx) => (Some(&mut ctx.m8), Some(&mut ctx.lbk)),
            ThreadContext::Worker => (None, None),
        }
    }
    pub fn lbk(&mut self) -> Option<&mut LiteralBookKeeping<Cdf16, AllocU8, AllocCDF16>> {
        match *self {
            ThreadContext::MainThread(ref mut ctx) => Some(&mut ctx.lbk),
            ThreadContext::Worker => None,
        }
    }
}
pub struct CrossCommandState<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                             Specialization:EncoderOrDecoderSpecialization,
                             LinearInputBytes:StreamDemuxer<AllocU8>,
                             LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                             Cdf16:CDF16,
                             AllocU8:Allocator<u8>,
                             AllocCDF16:Allocator<Cdf16>> {
    //pub cmd_coder: ArithmeticCoder,
    //pub lit_coder: ArithmeticCoder,
    pub coder: ArithmeticCoder,
    pub specialization: Specialization,
    pub thread_ctx: ThreadContext<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder>,
    pub bk: CrossCommandBookKeeping<Cdf16, AllocU8, AllocCDF16>,
    pub demuxer: LinearInputBytes,
    pub muxer: LinearOutputBytes,
}

impl<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                             Specialization:EncoderOrDecoderSpecialization,
                             LinearInputBytes:StreamDemuxer<AllocU8>,
                             LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                             Cdf16:CDF16,
                             AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<Cdf16>> CrossCommandState<ArithmeticCoder,
                                                    Specialization,
                                                    LinearInputBytes,
                                                    LinearOutputBytes,
                                                    Cdf16,
                                                    AllocU8,
                                                    AllocCDF16>{
    
    #[inline(always)]
    pub fn drain_or_fill_internal_buffer_lit(&mut self, output:&mut[u8], output_offset:&mut usize) -> DivansResult {
        let main = self.thread_ctx.main_thread_mut().unwrap();
        // FIXME(threading): do not call this
        drain_or_fill_static_buffer(LIT_CODER, &mut main.lit_coder, &mut self.demuxer, &mut self.muxer, output, output_offset,
                                    &mut Some(main.m8.get_base_alloc()))
    }
    #[inline(always)]
    pub fn drain_or_fill_internal_buffer_cmd(&mut self, output:&mut[u8], output_offset:&mut usize) -> DivansResult {
        match self.thread_ctx.main_thread_mut() {
            Some(ref mut main) => drain_or_fill_static_buffer(CMD_CODER, &mut self.coder, &mut self.demuxer, &mut self.muxer, output, output_offset,
                                                              &mut Some(main.m8.get_base_alloc())),
            None => drain_or_fill_static_buffer(CMD_CODER, &mut self.coder, &mut self.demuxer, &mut self.muxer, output, output_offset,
                                                &mut None), // FIXME: should we just return success if we are on the worker
        }
    }
}
impl <AllocU8:Allocator<u8>,
      LinearInputBytes:StreamDemuxer<AllocU8>,
      LinearOutputBytes:StreamMuxer<AllocU8>+Default,                                   
      Cdf16:CDF16,
      AllocCDF16:Allocator<Cdf16>,
      ArithmeticCoder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>,
      Specialization:EncoderOrDecoderSpecialization,
      > CrossCommandState<ArithmeticCoder,
                          Specialization,
                          LinearInputBytes,
                          LinearOutputBytes,
                          Cdf16,
                          AllocU8,
                          AllocCDF16> {
    pub fn new(mut m8: AllocU8,
               mut mcdf16:AllocCDF16,
               //cmd_coder: ArithmeticCoder,
               //lit_coder: ArithmeticCoder,
               cmd_coder: ArithmeticCoder,
               lit_coder: ArithmeticCoder,
               spc: Specialization,
               linear_input_bytes: LinearInputBytes,
               ring_buffer_size: usize,
               dynamic_context_mixing: u8,
               prior_depth:u8,
               literal_adaptation_rate: Option<[Speed;4]>,
               do_context_map:bool,
               force_stride: StrideSelection) -> Self {
        let ring_buffer = m8.alloc_cell(1 << ring_buffer_size);
        let lit_low_priors = mcdf16.alloc_cell(LiteralNibblePriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let lit_high_priors = mcdf16.alloc_cell(LiteralNibblePriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let lit_len_priors = mcdf16.alloc_cell(LiteralCommandPriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let copy_priors = mcdf16.alloc_cell(CopyCommandPriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let dict_priors = mcdf16.alloc_cell(DictCommandPriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let cc_priors = mcdf16.alloc_cell(CrossCommandPriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let pred_priors = mcdf16.alloc_cell(PredictionModePriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let btype_priors = mcdf16.alloc_cell(BlockTypePriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let literal_context_map = m8.alloc_cell(MAX_ADV_LITERAL_CONTEXT_MAP_SIZE);
        let distance_context_map = m8.alloc_cell(4 * NUM_BLOCK_TYPES);
        CrossCommandState::<ArithmeticCoder,
                            Specialization,
                            LinearInputBytes,
                            LinearOutputBytes,
                            Cdf16,
                            AllocU8,
                            AllocCDF16> {
            //cmd_coder: cmd_coder,
            coder: cmd_coder,
            //lit_coder: lit_coder,
            specialization: spc,
            thread_ctx: ThreadContext::MainThread(MainThreadContext::<Cdf16, AllocU8, AllocCDF16, ArithmeticCoder>{
                recoder: DivansRecodeState::<AllocU8::AllocatedMemory>::new(
                ring_buffer),
                m8: RepurposingAlloc::<u8, AllocU8>::new(m8),
                mcdf16:mcdf16,
                lbk: LiteralBookKeeping::new(literal_context_map),
                lit_high_priors: LiteralNibblePriors {
                    priors: lit_high_priors
                },
                lit_low_priors: LiteralNibblePriors {
                    priors: lit_low_priors
                },
                lit_coder: lit_coder,
            }),
            demuxer:linear_input_bytes,
            muxer:LinearOutputBytes::default(),
            bk:CrossCommandBookKeeping::new(lit_len_priors, cc_priors, copy_priors,
                                            dict_priors, pred_priors, btype_priors,
                                            distance_context_map,
                                            dynamic_context_mixing,
                                            prior_depth,
                                            literal_adaptation_rate,
                                            do_context_map,
                                            force_stride,
            ),
        }
    }
    fn free_internal(&mut self) {
        self.muxer.free_mux(self.thread_ctx.m8().unwrap().get_base_alloc());
        self.demuxer.free_demux(self.thread_ctx.m8().unwrap().get_base_alloc());
        self.bk.prediction_priors.summarize_speed_costs();
        self.bk.btype_priors.summarize_speed_costs();
        self.bk.cc_priors.summarize_speed_costs();
        self.bk.copy_priors.summarize_speed_costs();
        self.bk.dict_priors.summarize_speed_costs();
        if let ThreadContext::MainThread(ref mut ctx) = self.thread_ctx {
            ctx.lit_high_priors.summarize_speed_costs();
            ctx.lit_low_priors.summarize_speed_costs();
            ctx.lbk.lit_cm_priors.summarize_speed_costs();
        }
        let cdf16a = core::mem::replace(&mut self.bk.cc_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16b = core::mem::replace(&mut self.bk.copy_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16c = core::mem::replace(&mut self.bk.dict_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16d = core::mem::replace(&mut self.bk.btype_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16e = core::mem::replace(&mut self.bk.prediction_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16f = core::mem::replace(&mut self.bk.lit_len_priors.priors, AllocCDF16::AllocatedMemory::default());
        self.coder.free(self.thread_ctx.m8().unwrap().get_base_alloc());
        self.thread_ctx.m8().unwrap().get_base_alloc().free_cell(core::mem::replace(&mut self.bk.distance_context_map,
                                                              AllocU8::AllocatedMemory::default()));
        if let Some(mcdf16) = self.thread_ctx.mcdf16() {
            mcdf16.free_cell(cdf16a);
            mcdf16.free_cell(cdf16b);
            mcdf16.free_cell(cdf16c);
            mcdf16.free_cell(cdf16d);
            mcdf16.free_cell(cdf16e);
            mcdf16.free_cell(cdf16f);
        }
        match self.thread_ctx {
            ThreadContext::MainThread(ref mut ctx) => {
                ctx.lit_coder.free(ctx.m8.get_base_alloc());
                ctx.free()
            },
            ThreadContext::Worker => {},
        }
    }
    pub fn free_ref(&mut self) {
        self.free_internal();
        self.thread_ctx.m8().unwrap().free_ref();
    }
    pub fn free(mut self) -> (AllocU8, AllocCDF16) {
        self.free_internal();
        let mt = match self.thread_ctx {
            ThreadContext::MainThread(x) => x,
            ThreadContext::Worker => unreachable!(),
        };
        (mt.m8.free(), mt.mcdf16)
    }
}

#[inline(always)]
pub fn drain_or_fill_static_buffer<AllocU8:Allocator<u8>,
                                   ArithmeticCoder:ArithmeticEncoderOrDecoder,
                                   LinearInputBytes:StreamDemuxer<AllocU8>,
                                   LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                                   >(stream_index: usize,
                                     local_coder: &mut ArithmeticCoder,
                                     demuxer: &mut LinearInputBytes,
                                     muxer: &mut LinearOutputBytes,
                                     output_bytes: &mut[u8],
                                     output_offset: &mut usize,
                                     m8:&mut Option<&mut AllocU8>) -> DivansResult {
    if LinearOutputBytes::can_linearize() {
        while local_coder.has_data_to_drain_or_fill() {
            *output_offset += muxer.linearize(output_bytes.split_at_mut(*output_offset).1);
            let mut cur_input = demuxer.read_buffer();
            let mut cur_output = muxer.write_buffer(match m8 {Some(ref mut x) => x, None => unreachable!()});
            match local_coder.drain_or_fill_internal_buffer_unchecked(&mut cur_input[stream_index], &mut cur_output[stream_index]) {
                DivansResult::NeedsMoreOutput => {
                    assert!(LinearOutputBytes::can_linearize());
                    if *output_offset == output_bytes.len() {
                        return DivansResult::NeedsMoreOutput;
                    }
                },
                res => {
                    return res;
                },
            }
        }
        DivansResult::Success
    } else {
        if local_coder.has_data_to_drain_or_fill() {
            let mut cur_input = demuxer.read_buffer();
            let mut a = 0usize;
            let mut b = 0usize;
            let mut cur_output = [
                WritableBytes{
                    data:&mut [],
                    write_offset:&mut a,
                },
                WritableBytes{
                data:&mut [],
                    write_offset:&mut b,
                },
            ];
            local_coder.drain_or_fill_internal_buffer_unchecked(&mut cur_input[stream_index], &mut cur_output[stream_index])
        } else {
            DivansResult::Success
        }
    }
}
