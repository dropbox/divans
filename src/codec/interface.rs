use core;
use brotli::BrotliResult;
use ::cmd_to_raw::DivansRecodeState;
use ::probability::{CDF2, CDF16, Speed};
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use ::slice_util::AllocatedMemoryPrefix;
use ::alloc_util::RepurposingAlloc;
use ::constants;
use ::interface::{
    ArithmeticEncoderOrDecoder,
    CrossCommandBilling,
    Command,
    CopyCommand,
    DictCommand,
    LiteralCommand,
    LiteralBlockSwitch,
    LiteralPredictionModeNibble,
    LITERAL_PREDICTION_MODE_SIGN,
    LITERAL_PREDICTION_MODE_UTF8,
    LITERAL_PREDICTION_MODE_MSB6,
    LITERAL_PREDICTION_MODE_LSB6,
    NewWithAllocator,
};
use super::priors::{
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
const LOG_NUM_COPY_TYPE_PRIORS: usize = 2;
const LOG_NUM_DICT_TYPE_PRIORS: usize = 2;
pub const BLOCK_TYPE_LITERAL_SWITCH:usize=0;
pub const BLOCK_TYPE_COMMAND_SWITCH:usize=1;
pub const BLOCK_TYPE_DISTANCE_SWITCH:usize=2;

#[derive(Clone, Copy)]
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

#[allow(non_snake_case)]
pub fn Fail() -> BrotliResult {
    BrotliResult::ResultFailure
}






#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ContextMapType {
    Literal,
    Distance
}


#[derive(Copy,Clone)]
pub struct DistanceCacheEntry {
    pub distance:u32,
    pub decode_byte_count:u32,
}

const CONTEXT_MAP_CACHE_SIZE: usize = 13;

pub struct CrossCommandBookKeeping<Cdf16:CDF16,
                                   AllocU8:Allocator<u8>,
                                   AllocCDF2:Allocator<CDF2>,
                                   AllocCDF16:Allocator<Cdf16>> {
    pub model_weights: [super::weights::Weights;2],
    pub last_8_literals: u64,
    pub decode_byte_count: u32,
    pub command_count:u32,
    pub num_literals_coded: u32,
    pub literal_context_map: AllocU8::AllocatedMemory,
    pub distance_context_map: AllocU8::AllocatedMemory,
    pub lit_priors: LiteralCommandPriors<Cdf16, AllocCDF16>,
    pub lit_cm_priors: LiteralCommandPriorsCM<Cdf16, AllocCDF16>,
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
    pub stride: u8,
    pub cm_prior_depth_mask: u8,
    pub prior_bytes_depth_mask: u8,
    pub second_nibble_depth_mask: u8,
    pub last_dlen: u8,
    pub last_clen: u8,
    pub last_llen: u32,
    pub last_4_states: u8,
    pub materialized_context_map: bool,
    pub combine_literal_predictions: bool,
    pub desired_prior_depth: u8,
    pub desired_context_mixing: u8,
    pub literal_prediction_mode: LiteralPredictionModeNibble,
    pub literal_adaptation: Speed,
    pub desired_literal_adaptation: Speed,
    pub desired_do_context_map: bool,
    pub desired_force_stride: StrideSelection,
    pub literal_lut0:[u8;256],
    pub literal_lut1:[u8;256],
    _legacy: core::marker::PhantomData<AllocCDF2>,
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
  pub actual_context: u8,
  pub stride_byte: u8,
  pub stride_second_byte: u8,
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

impl<Cdf16:CDF16,
     AllocCDF2:Allocator<CDF2>,
     AllocCDF16:Allocator<Cdf16>,
     AllocU8:Allocator<u8>> CrossCommandBookKeeping<Cdf16,
                                                    AllocU8,
                                                    AllocCDF2,
                                                    AllocCDF16> {
    fn new(lit_prior: AllocCDF16::AllocatedMemory,
           cm_lit_prior: AllocCDF16::AllocatedMemory,
           cc_prior: AllocCDF16::AllocatedMemory,
           copy_prior: AllocCDF16::AllocatedMemory,
           dict_prior: AllocCDF16::AllocatedMemory,
           pred_prior: AllocCDF16::AllocatedMemory,
           btype_prior: AllocCDF16::AllocatedMemory,
           literal_context_map: AllocU8::AllocatedMemory,
           distance_context_map: AllocU8::AllocatedMemory,
           mut dynamic_context_mixing: u8,
           prior_depth: u8,
           literal_adaptation_speed:Speed,
           do_context_map: bool,
           force_stride: StrideSelection) -> Self {
        assert!(dynamic_context_mixing < 15); // leaves room for expansion
        match force_stride {
            StrideSelection::PriorDisabled => {},
            _ => if dynamic_context_mixing == 0 && do_context_map {
                dynamic_context_mixing = 1; // make sure to mix if user requested both context map and stride
            },
        }
        let mut ret = CrossCommandBookKeeping{
            model_weights:[super::weights::Weights::default(),
                           super::weights::Weights::default()],
            cm_prior_depth_mask:0xff,
            prior_bytes_depth_mask:0x0,
            second_nibble_depth_mask:0xff,
            desired_prior_depth:prior_depth,
            desired_literal_adaptation: literal_adaptation_speed,
            desired_context_mixing:dynamic_context_mixing,
            literal_adaptation: default_literal_speed(),
            decode_byte_count:0,
            command_count:0,
            num_literals_coded:0,
            distance_cache:[
                [
                    DistanceCacheEntry{
                        distance:1,
                        decode_byte_count:0,
                    };3];32],
            stride: 0,
            last_dlen: 1,
            last_llen: 1,
            last_clen: 1,
            materialized_context_map: false,
            combine_literal_predictions: false,
            last_4_states: 3 << (8 - LOG_NUM_COPY_TYPE_PRIORS),
            last_8_literals: 0,
            literal_prediction_mode: LiteralPredictionModeNibble::default(),
            literal_lut0: get_lut0(LiteralPredictionModeNibble::default()),
            literal_lut1: get_lut1(LiteralPredictionModeNibble::default()),
            cmap_lru: [0u8; CONTEXT_MAP_CACHE_SIZE],
            prediction_priors: PredictionModePriors {
                priors: pred_prior,
            },
            lit_cm_priors: LiteralCommandPriorsCM {
                priors: cm_lit_prior
            },
            lit_priors: LiteralCommandPriors {
                priors: lit_prior
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
            literal_context_map:literal_context_map,
            distance_context_map:distance_context_map,
            btype_priors: BlockTypePriors {
                priors: btype_prior
            },
            distance_lru: [4,11,15,16],
            btype_lru:[[0,1];3],
            btype_max_seen:[0;3],
            desired_do_context_map: do_context_map,
            desired_force_stride:force_stride,
            _legacy: core::marker::PhantomData::<AllocCDF2>::default(),
        };
        for i in 0..4 {
            for j in 0..0x10 {
                let prob = ret.cc_priors.get(CrossCommandBilling::FullSelection,
                                             (i, j));
                if j == 0x3 { // starting situation
                    prob.blend(0x7, Speed::ROCKET);
                } else {
                    prob.blend(0x1, Speed::FAST);
                    prob.blend(0x1, Speed::FAST);
                    prob.blend(0x2, Speed::FAST);
                    prob.blend(0x1, Speed::FAST);
                    prob.blend(0x1, Speed::FAST);
                    prob.blend(0x1, Speed::FAST);
                    prob.blend(0x2, Speed::FAST);
                    prob.blend(0x3, Speed::FAST);
                    prob.blend(0x3, Speed::FAST);
                }
            }
        }
        ret
    }
    pub fn materialized_prediction_mode(&self) -> bool {
        self.materialized_context_map
    }
    pub fn obs_literal_adaptation_rate(&mut self, ladaptation_rate: Speed) {
        self.literal_adaptation = ladaptation_rate;
    }
    pub fn obs_prior_depth(&mut self, prior_depth: u8) {
        //self.prior_depth = prior_depth;
        match prior_depth {
            0 | 1 | 2 | 3 | 4 => {
                self.cm_prior_depth_mask = 0xff;
                self.prior_bytes_depth_mask = 0x0;
                self.second_nibble_depth_mask = 0xf0 | ((1 << prior_depth) - 1);
            },
            5 => {
                self.cm_prior_depth_mask = 0x0;
                self.prior_bytes_depth_mask = 0xff;
                self.second_nibble_depth_mask = 0xf3;
            },
            6 => {
                self.cm_prior_depth_mask = 0x0;
                self.prior_bytes_depth_mask = 0xff;
                self.second_nibble_depth_mask = 0xf7;
            },
            7 => {
                self.cm_prior_depth_mask = 0x0;
                self.prior_bytes_depth_mask = 0xff;
                self.second_nibble_depth_mask = 0xff;
            },
            8 => {
                self.cm_prior_depth_mask = 0x7f;
                self.prior_bytes_depth_mask = 0x8 << 4;
                self.second_nibble_depth_mask = 0xff;
            },
            9 => {
                self.cm_prior_depth_mask = 0x3f;
                self.prior_bytes_depth_mask = 0xc << 4;
                self.second_nibble_depth_mask = 0xff;
            },
            0xa => {
                self.cm_prior_depth_mask = 0x1f;
                self.prior_bytes_depth_mask = 0xe << 4;
                self.second_nibble_depth_mask = 0xff;
            },
            0xb => {
                self.cm_prior_depth_mask = 0xf;
                self.prior_bytes_depth_mask = 0xf << 4;
                self.second_nibble_depth_mask = 0xff;
            },
            0xc => {
                self.cm_prior_depth_mask = 0x7;
                self.prior_bytes_depth_mask = 0xf8;
                self.second_nibble_depth_mask = 0xff;
            },
            0xd => {
                self.cm_prior_depth_mask = 0x3;
                self.prior_bytes_depth_mask = 0xfc;
                self.second_nibble_depth_mask = 0xff;
            },
            0xe => {
                self.cm_prior_depth_mask = 0x1;
                self.prior_bytes_depth_mask = 0xfe;
                self.second_nibble_depth_mask = 0xff;
            },
            0xf => {
                self.cm_prior_depth_mask = 0x0;
                self.prior_bytes_depth_mask = 0xff;
                self.second_nibble_depth_mask = 0xff;
            },
            _ => panic!("not possible"),
        }
    }
    pub fn obs_dynamic_context_mixing(&mut self, context_mixing: u8) {
        self.combine_literal_predictions = (context_mixing != 0) as bool;
        self.model_weights[0].set_mixing_param(context_mixing);
        self.model_weights[1].set_mixing_param(context_mixing);
    }

    pub fn get_distance_prior(&mut self, copy_len: u32) -> usize {
        let dtype = self.get_distance_block_type();
        let distance_map_index = dtype as usize * 4 + core::cmp::min(copy_len as usize - 1, 3);
        self.distance_context_map.slice()[distance_map_index] as usize
    }
    pub fn reset_context_map_lru(&mut self) {
        self.cmap_lru = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    }
    pub fn obs_context_map(&mut self, context_map_type: ContextMapType, index : u32, val: u8) -> BrotliResult {
        self.materialized_context_map = true;
        let target_array = match context_map_type {
            ContextMapType::Literal => self.literal_context_map.slice_mut(),
            ContextMapType::Distance=> self.distance_context_map.slice_mut(),
        };
        if index as usize >= target_array.len() {
            return           BrotliResult::ResultFailure;
        }

        target_array[index as usize] = val;
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
        BrotliResult::ResultSuccess
    }
    pub fn read_distance_cache(&self, len:u32, index:u32) -> u32 {
        let len_index = core::cmp::min(len as usize, self.distance_cache.len() - 1);
        self.distance_cache[len_index][index as usize].distance + (
            self.decode_byte_count - self.distance_cache[len_index][index as usize].decode_byte_count)
    }
    pub fn get_distance_from_mnemonic_code_two(&self, code:u8, len:u32,) -> u32 {
        match code {
            0 => sub_or_add(self.distance_lru[2], 1, 3),
            1 => self.read_distance_cache(len, 0),
            2 => self.read_distance_cache(len, 1),
            3 => self.read_distance_cache(len, 2),
            4 => self.read_distance_cache(len + 1, 0),
            5 => self.read_distance_cache(len + 1, 1),
            6 => self.read_distance_cache(len + 1, 2),
            7 => self.read_distance_cache(len + 1, 0) - 1,
            8 => self.read_distance_cache(len + 1, 1) - 1,
            9 => self.read_distance_cache(len + 1, 2) - 1,
            10 => self.read_distance_cache(len + 2, 0),
            11 => self.read_distance_cache(len + 2, 1),
            12 => self.read_distance_cache(len + 2, 2),
            13 => self.read_distance_cache(len + 2, 0) - 1,
            14 => self.read_distance_cache(len + 2, 1) - 1,
            _ => panic!("Logic error: nibble > 14 evaluated for nmemonic"),
        }
    }
    pub fn distance_mnemonic_code_two(&self, d: u32, len:u32) -> u8 {
        for i in 0..15 {
            if self.get_distance_from_mnemonic_code_two(i as u8, len) == d {
                return i as u8;
            }
        }
        15
    }
    #[inline(always)]
    pub fn get_distance_from_mnemonic_code(&self, code:u8) -> u32 {
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
            return self.distance_lru[code as usize]; // less than four is a fetch
        }
        let unsigned_summand = (code >> 2) as i32; // greater than four either adds or subtracts
        // the value depending on if its an even or odd code
        // mnemonic 1 are codes that have bit 2 set, mnemonic 0 are codes that don't have bit 2 set
        let signed_summand = unsigned_summand - (((-(code as i32 & 1)) & unsigned_summand) << 1);
        ((self.distance_lru[((code & 2) >> 1) as usize] as i32) + signed_summand) as u32
    }
    pub fn distance_mnemonic_code(&self, d: u32) -> u8 {
        for i in 0..15 {
            if self.get_distance_from_mnemonic_code(i as u8) == d {
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
    pub fn push_literal_nibble(&mut self, nibble: u8) {
        self.last_8_literals >>= 0x4;
        self.last_8_literals |= u64::from(nibble) << 0x3c;
    }
    pub fn push_literal_byte(&mut self, b: u8) {
        self.num_literals_coded += 1;
        self.last_8_literals >>= 0x8;
        self.last_8_literals |= u64::from(b) << 0x38;
    }
    pub fn get_command_type_prob(&mut self) -> &mut Cdf16 {
        //let last_8 = self.cross_command_state.recoder.last_8_literals();
        self.cc_priors.get(CrossCommandBilling::FullSelection,
                           ((self.last_4_states as usize) >> (8 - LOG_NUM_COPY_TYPE_PRIORS),
                           ((self.last_8_literals>>0x3e) as usize &0xf)))
    }
    fn next_state(&mut self) {
        self.last_4_states >>= 2;
    }
    pub fn obs_pred_mode(&mut self, new_mode: LiteralPredictionModeNibble) {
       self.next_state();
       self.literal_prediction_mode = new_mode;
       self.literal_lut0 = get_lut0(new_mode);
       self.literal_lut1 = get_lut1(new_mode);
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
            let mut sub_index = if self.distance_cache[nb][1].decode_byte_count < self.distance_cache[nb][0].decode_byte_count {
                1
            } else {
                0
            };
            if self.distance_cache[nb][2].decode_byte_count < self.distance_cache[nb][sub_index].decode_byte_count {
                sub_index = 2;
            }
            self.distance_cache[nb][sub_index] = DistanceCacheEntry{
                distance: 0,//cc.distance, we're copying it to here (ha!)
                decode_byte_count:self.decode_byte_count,
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
        self.stride = btype.stride();
        if self.stride != 0 && self.materialized_context_map {
            debug_assert!(self.combine_literal_predictions);
        }
    }
    pub fn obs_btypec(&mut self, btype:u8) {
        self._obs_btype_helper(BLOCK_TYPE_COMMAND_SWITCH, btype);
    }
    pub fn obs_btyped(&mut self, btype:u8) {
        self._obs_btype_helper(BLOCK_TYPE_DISTANCE_SWITCH, btype);
    }
}

pub struct CrossCommandState<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                             Specialization:EncoderOrDecoderSpecialization,
                             Cdf16:CDF16,
                             AllocU8:Allocator<u8>,
                             AllocCDF2:Allocator<CDF2>,
                             AllocCDF16:Allocator<Cdf16>> {
    pub coder: ArithmeticCoder,
    pub specialization: Specialization,
    pub recoder: DivansRecodeState<AllocU8::AllocatedMemory>,
    pub m8: RepurposingAlloc<u8, AllocU8>,
    mcdf2: AllocCDF2,
    mcdf16: AllocCDF16,
    pub bk: CrossCommandBookKeeping<Cdf16, AllocU8, AllocCDF2, AllocCDF16>,
}

impl <AllocU8:Allocator<u8>,
      Cdf16:CDF16,
      AllocCDF2:Allocator<CDF2>,
      AllocCDF16:Allocator<Cdf16>,
      ArithmeticCoder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>,
      Specialization:EncoderOrDecoderSpecialization,
      > CrossCommandState<ArithmeticCoder,
                          Specialization,
                          Cdf16,
                          AllocU8,
                          AllocCDF2,
                          AllocCDF16> {
    pub fn new(mut m8: AllocU8,
               mcdf2:AllocCDF2,
               mut mcdf16:AllocCDF16,
               coder: ArithmeticCoder,
               spc: Specialization,
               ring_buffer_size: usize,
               dynamic_context_mixing: u8,
               prior_depth:u8,
               literal_adaptation_rate :Speed,
               do_context_map:bool,
               force_stride: StrideSelection) -> Self {
        let ring_buffer = m8.alloc_cell(1 << ring_buffer_size);
        let lit_priors = mcdf16.alloc_cell(LiteralCommandPriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let cm_lit_prior = mcdf16.alloc_cell(LiteralCommandPriorsCM::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let copy_priors = mcdf16.alloc_cell(CopyCommandPriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let dict_priors = mcdf16.alloc_cell(DictCommandPriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let cc_priors = mcdf16.alloc_cell(CrossCommandPriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let pred_priors = mcdf16.alloc_cell(PredictionModePriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let btype_priors = mcdf16.alloc_cell(BlockTypePriors::<Cdf16, AllocCDF16>::NUM_ALL_PRIORS);
        let literal_context_map = m8.alloc_cell(64 * NUM_BLOCK_TYPES);
        let distance_context_map = m8.alloc_cell(4 * NUM_BLOCK_TYPES);
        CrossCommandState::<ArithmeticCoder,
                            Specialization,
                            Cdf16,
                            AllocU8,
                            AllocCDF2,
                            AllocCDF16> {
            coder: coder,
            specialization: spc,
            recoder: DivansRecodeState::<AllocU8::AllocatedMemory>::new(
                ring_buffer),
            m8: RepurposingAlloc::<u8, AllocU8>::new(m8),
            mcdf2:mcdf2,
            mcdf16:mcdf16,
            bk:CrossCommandBookKeeping::new(lit_priors, cm_lit_prior,
                                            cc_priors, copy_priors,
                                            dict_priors, pred_priors, btype_priors,
                                            literal_context_map, distance_context_map,
                                            dynamic_context_mixing,
                                            prior_depth,
                                            literal_adaptation_rate,
                                            do_context_map,
                                            force_stride,
            ),
        }
    }
    fn free_internal(&mut self) {
        let rb = core::mem::replace(&mut self.recoder.ring_buffer, AllocU8::AllocatedMemory::default());
        let cdf16a = core::mem::replace(&mut self.bk.cc_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16b = core::mem::replace(&mut self.bk.copy_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16c = core::mem::replace(&mut self.bk.dict_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16d = core::mem::replace(&mut self.bk.lit_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16e = core::mem::replace(&mut self.bk.lit_cm_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16f = core::mem::replace(&mut self.bk.btype_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16g = core::mem::replace(&mut self.bk.prediction_priors.priors, AllocCDF16::AllocatedMemory::default());
        self.coder.free(self.m8.get_base_alloc());
        self.m8.get_base_alloc().free_cell(core::mem::replace(&mut self.bk.literal_context_map,
                                                              AllocU8::AllocatedMemory::default()));
        self.m8.get_base_alloc().free_cell(core::mem::replace(&mut self.bk.distance_context_map,
                                                              AllocU8::AllocatedMemory::default()));
        self.m8.free_cell(rb);
        self.mcdf16.free_cell(cdf16a);
        self.mcdf16.free_cell(cdf16b);
        self.mcdf16.free_cell(cdf16c);
        self.mcdf16.free_cell(cdf16d);
        self.mcdf16.free_cell(cdf16e);
        self.mcdf16.free_cell(cdf16f);
        self.mcdf16.free_cell(cdf16g);
    }
    pub fn free_ref(&mut self) {
        self.free_internal();
        self.m8.free_ref();
    }
    pub fn free(mut self) -> (AllocU8, AllocCDF2, AllocCDF16) {
        self.free_internal();
        (self.m8.free(), self.mcdf2, self.mcdf16)
    }
}
