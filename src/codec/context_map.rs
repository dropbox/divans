use brotli::BrotliResult;
use super::interface::ContextMapType;
use super::priors::{PredictionModePriorType};
use alloc::{Allocator, SliceWrapper};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
};
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    LiteralPredictionModeNibble,
    PredictionModeContextMap,
};
use ::priors::PriorCollection;
use ::probability::{Speed, CDF2, CDF16};
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PredictionModeState {
    Begin,
    DynamicContextMixing,
    PriorDepth,
    LiteralAdaptationRate,
    ContextMapMnemonic(u32, ContextMapType),
    ContextMapFirstNibble(u32, ContextMapType),
    ContextMapSecondNibble(u32, ContextMapType, u8),
    ContextMapSpeedLow(u32, u8),
    ContextMapSpeedHigh(u32, u8),
    FullyDecoded,
}


fn next_mul_512(a: usize) -> usize {
    a + (512 - (a & 511))
}

fn all_same(data: &[u8], goal: u8) -> bool {
    if data.len() == 0 {
        return true;
    }
    if data[data.len() - 1] != goal {
        return false;
    }
    for i in data.iter() {
        if *i != goal {
            return false;
        }
    }
    true
}

impl PredictionModeState {
    pub fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                        Specialization:EncoderOrDecoderSpecialization,
                        Cdf16:CDF16,
                        AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<CDF2>,
                        AllocCDF16:Allocator<Cdf16>,
                        SliceType:SliceWrapper<u8>+Default>(&mut self,
                                               superstate: &mut CrossCommandState<ArithmeticCoder,
                                                                                  Specialization,
                                                                                  Cdf16,
                                                                                  AllocU8,
                                                                                  AllocCDF2,
                                                                                  AllocCDF16>,
                                               in_cmd: &PredictionModeContextMap<SliceType>,
                                               input_bytes:&[u8],
                                               input_offset: &mut usize,
                                               output_bytes:&mut [u8],
                                               output_offset: &mut usize) -> BrotliResult {
        let mut speed_index_offset = 0;
        let mut speed_index_mask = 0;
        let max_speed_index = 2048;
        loop {
            match superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset) {
                BrotliResult::ResultSuccess => {},
                need_something => return need_something,
            }
            let billing = BillingDesignation::PredModeCtxMap(match *self {
                PredictionModeState::ContextMapMnemonic(
                    _, context_map_type) => PredictionModeState::ContextMapMnemonic(0,
                                                                                    context_map_type),
                PredictionModeState::ContextMapFirstNibble(
                    _, context_map_type) => PredictionModeState::ContextMapFirstNibble(0,
                                                                                                  context_map_type),
                PredictionModeState::ContextMapSecondNibble(
                    _, context_map_type, _) => PredictionModeState::ContextMapSecondNibble(0,
                                                                                                      context_map_type,
                                                                                                      0),
                PredictionModeState::ContextMapSpeedLow(
                    index, _) => PredictionModeState::ContextMapSpeedLow((index >> 9) & 1,
                                                                         index as u8 & 1),
                PredictionModeState::ContextMapSpeedHigh(
                    index, _) => PredictionModeState::ContextMapSpeedHigh((index >> 9) & 1,
                                                                         index as u8 & 1),
                a => a,
            });

            match *self {
               PredictionModeState::Begin => {
                   superstate.bk.reset_context_map_lru();
                   let mut beg_nib = in_cmd.literal_prediction_mode.prediction_mode();
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::Only, (0,));
                       superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                       nibble_prob.blend(beg_nib, Speed::MED);
                   }
                   let pred_mode = match LiteralPredictionModeNibble::new(beg_nib) {
                      Err(_) => return BrotliResult::ResultFailure,
                      Ok(pred_mode) => pred_mode,
                   };
                   superstate.bk.obs_pred_mode(pred_mode);
                   *self = PredictionModeState::DynamicContextMixing;
               },
               PredictionModeState::DynamicContextMixing => {
                   let mut beg_nib = superstate.bk.desired_context_mixing;
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(
                           PredictionModePriorType::DynamicContextMixingSpeed, (0,));
                       superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                       nibble_prob.blend(beg_nib, Speed::MED);
                   }
                   superstate.bk.obs_dynamic_context_mixing(beg_nib);
                   *self = PredictionModeState::PriorDepth;
               },
               PredictionModeState::PriorDepth => {
                   let mut beg_nib = superstate.bk.desired_prior_depth;
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(
                           PredictionModePriorType::PriorDepth, (0,));
                       superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                       nibble_prob.blend(beg_nib, Speed::FAST);
                   }
                   superstate.bk.obs_prior_depth(beg_nib);
                   *self = PredictionModeState::LiteralAdaptationRate;
               },
               PredictionModeState::LiteralAdaptationRate => {
                   let mut beg_nib = match superstate.bk.desired_literal_adaptation.clone() {
                       Speed::GEOLOGIC => GEOLOGIC_CODE,
                       Speed::GLACIAL => GLACIAL_CODE,
                       Speed::MUD => MUD_CODE,
                       Speed::SLOW => SLOW_CODE,
                       Speed::MED => MED_CODE,
                       Speed::FAST => FAST_CODE,
                       Speed::PLANE => PLANE_CODE,
                       Speed::ROCKET => ROCKET_CODE,
                       _ => return BrotliResult::ResultFailure,
                   };
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::LiteralSpeed, (0,));
                       superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                       nibble_prob.blend(beg_nib, Speed::MED);
                   }
                   const GEOLOGIC_CODE: u8 = 0;//Speed::GEOLOGIC as u8;
                   const GLACIAL_CODE: u8 = 1;//Speed::GLACIAL as u8;
                   const MUD_CODE:   u8 = 2;//Speed::MUD as u8;
                   const SLOW_CODE: u8 = 3;//Speed::SLOW as u8;
                   const MED_CODE: u8 = 4;//Speed::MED as u8;
                   const FAST_CODE: u8 = 5;//Speed::FAST as u8;
                   const PLANE_CODE: u8 = 6;//Speed::PLANE as u8;
                   const ROCKET_CODE: u8 = 7;//Speed::ROCKET as u8;
                   superstate.bk.obs_literal_adaptation_rate(match beg_nib {
                       GEOLOGIC_CODE => Speed::GEOLOGIC,
                       GLACIAL_CODE => Speed::GLACIAL,
                       MUD_CODE => Speed::MUD,
                       SLOW_CODE => Speed::SLOW,
                       MED_CODE => Speed::MED,
                       FAST_CODE => Speed::FAST,
                       PLANE_CODE => Speed::PLANE,
                       ROCKET_CODE => Speed::ROCKET,
                       _ => return BrotliResult::ResultFailure,
                   });
                   *self = PredictionModeState::ContextMapMnemonic(0, ContextMapType::Literal);
               },
               PredictionModeState::ContextMapMnemonic(index, context_map_type) => {
                   let mut cur_context_map = match context_map_type {
                           ContextMapType::Literal => in_cmd.literal_context_map.slice(),
                           ContextMapType::Distance => in_cmd.distance_context_map.slice(),
                   };
                   if !superstate.bk.desired_do_context_map {
                       cur_context_map = &cur_context_map[..0];
                   }
                       
                   let mut mnemonic_nibble = if index as usize >= cur_context_map.len() {
                       // encode nothing
                       14 // eof
                   } else {
                       let target_val = cur_context_map[index as usize];

                       let mut res = 15u8; // fallback
                       for (index, val) in superstate.bk.cmap_lru.iter().enumerate() {
                           if *val == target_val {
                               res = index as u8;
                           }
                       }
                       if target_val == superstate.bk.cmap_lru.iter().max().unwrap().wrapping_add(1) {
                           res = 13;
                       }
                       res
                   };
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::Mnemonic, (superstate.bk.last_cm_mnemonic as usize,));
                       superstate.coder.get_or_put_nibble(&mut mnemonic_nibble, nibble_prob, billing);
                       nibble_prob.blend(mnemonic_nibble, Speed::MUD);
                   }
                   superstate.bk.last_cm_mnemonic = mnemonic_nibble;
                   if mnemonic_nibble == 14 {
                       match context_map_type {
                           ContextMapType::Literal => { // switch to distance context map
                               superstate.bk.reset_context_map_lru(); // distance context map should start with 0..14 as lru
                               *self = PredictionModeState::ContextMapMnemonic(0, ContextMapType::Distance);
                           },
                           ContextMapType::Distance => { // finished
                               *self = PredictionModeState::ContextMapSpeedHigh(0, 0);
                           }
                       }
                   } else if mnemonic_nibble == 15 {
                       *self = PredictionModeState::ContextMapFirstNibble(index, context_map_type);
                   } else {
                       let val = if mnemonic_nibble == 13 {
                           superstate.bk.cmap_lru.iter().max().unwrap().wrapping_add(1)
                       } else {
                           superstate.bk.cmap_lru[mnemonic_nibble as usize]
                       };
                       if let BrotliResult::ResultFailure = superstate.bk.obs_context_map(context_map_type, index, val) {
                           return BrotliResult::ResultFailure;
                       }
                       *self = PredictionModeState::ContextMapMnemonic(index + 1, context_map_type);
                   }
               },
               PredictionModeState::ContextMapFirstNibble(index, context_map_type) => {
                   let cur_context_map = match context_map_type {
                       ContextMapType::Literal => in_cmd.literal_context_map.slice(),
                       ContextMapType::Distance => in_cmd.distance_context_map.slice(),
                   };
                   let mut msn_nib = if index as usize >= cur_context_map.len() {
                       // encode nothing
                       0
                   } else {
                       cur_context_map[index as usize] >> 4
                   };
                   let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::FirstNibble,
                                                                             ((superstate.bk.last_cm_byte as usize )>> 4,));

                   superstate.coder.get_or_put_nibble(&mut msn_nib, nibble_prob, billing);
                   nibble_prob.blend(msn_nib, Speed::MUD);
                   *self = PredictionModeState::ContextMapSecondNibble(index, context_map_type, msn_nib);
               },
               PredictionModeState::ContextMapSecondNibble(index, context_map_type, most_significant_nibble) => {
                   let cur_context_map = match context_map_type {
                       ContextMapType::Literal => in_cmd.literal_context_map.slice(),
                       ContextMapType::Distance => in_cmd.distance_context_map.slice(),
                   };
                   let mut lsn_nib = if index as usize >= cur_context_map.len() {
                       // encode nothing
                       0
                   } else {
                       cur_context_map[index as usize] & 0xf
                   };
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::SecondNibble,
                                                                                 (superstate.bk.last_cm_byte as usize & 0xf,));
                       // could put first_nibble as ctx instead of 0, but that's probably not a good idea since we never see
                       // the same nibble twice in all likelihood if it was covered by the mnemonic--unless we want random (possible?)
                       superstate.coder.get_or_put_nibble(&mut lsn_nib, nibble_prob, billing);
                       nibble_prob.blend(lsn_nib, Speed::MUD);
                   }
                   if let BrotliResult::ResultFailure = superstate.bk.obs_context_map(context_map_type, index, (most_significant_nibble << 4) | lsn_nib) {
                       return BrotliResult::ResultFailure;
                   }
                   *self = PredictionModeState::ContextMapMnemonic(index + 1, context_map_type);
               },
               PredictionModeState::ContextMapSpeedHigh(index, last_byte) => {
                   if index == 0 {
                       if superstate.bk.combine_literal_predictions {
                           speed_index_offset = 1024;
                           speed_index_mask = 0xffff;
                       } else {
                           speed_index_offset = 1024;
                           speed_index_mask = 0x7ff; // so we wrap around and pick up vanilla stride instead of the combination
                       }
                   }
                   let speeds = in_cmd.context_speeds.slice();
                   let mut msn_nib = if index as usize >= speeds.len() {
                       0xf
                   } else {
                       let input_index = (index as usize + speed_index_offset) & speed_index_mask;
                       if (index >= 1024 &&
                           superstate.bk.materialized_context_map &&
                           !superstate.bk.combine_literal_predictions
                       ) || all_same(&speeds[input_index as usize..next_mul_512(input_index as usize)], last_byte) {
                           0xf
                       } else{
                           (speeds[input_index] >> 3) & 0xf
                       }
                   };
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::ContextMapSpeedHigh,
                                                                                 ((index as usize) >> 9, index as usize & 1,)); // FIXME: maybe need to use prev nibble
                       // could put first_nibble as ctx instead of 0, but that's probably not a good idea since we never see
                       // the same nibble twice in all likelihood if it was covered by the mnemonic--unless we want random (possible?)
                       superstate.coder.get_or_put_nibble(&mut msn_nib, nibble_prob, billing);
                       nibble_prob.blend(msn_nib, Speed::ROCKET);
                   }
                   if msn_nib == 0xf { // all the same
                       for i in index as usize..next_mul_512(index as usize) {
                           superstate.bk.obs_literal_speed(i as u32, in_cmd.f8_to_u16(last_byte));
                       }
                       let destination = next_mul_512(index as usize) as u32;
                       if destination < max_speed_index {
                           *self = PredictionModeState::ContextMapSpeedHigh(destination, last_byte);
                       } else {
                           *self = PredictionModeState::FullyDecoded;
                       }
                   } else {
                       *self = PredictionModeState::ContextMapSpeedLow(index, msn_nib)
                   }
               }
                PredictionModeState::ContextMapSpeedLow(index, msn_nib) => {
                   let speeds = in_cmd.context_speeds.slice();
                   let mut lsn_nib = if index as usize >= speeds.len() {
                       0x7
                   } else {
                       speeds[(index as usize + speed_index_offset) & speed_index_mask] & 0x7
                   };
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::ContextMapSpeedLow,
                                                                                 ((index as usize) >> 9, index as usize & 1, msn_nib as usize));
                       // could put first_nibble as ctx instead of 0, but that's probably not a good idea since we never see
                       // the same nibble twice in all likelihood if it was covered by the mnemonic--unless we want random (possible?)
                       superstate.coder.get_or_put_nibble(&mut lsn_nib, nibble_prob, billing);
                       nibble_prob.blend(msn_nib, Speed::ROCKET);
                   }
                   let last_byte = (msn_nib << 3) | lsn_nib;
                   superstate.bk.obs_literal_speed(index, in_cmd.f8_to_u16(last_byte));
                   if index == max_speed_index {
                       *self = PredictionModeState::FullyDecoded;
                   } else {
                       *self = PredictionModeState::ContextMapSpeedHigh(index + 1, last_byte);
                   }
               },
               PredictionModeState::FullyDecoded => {
                   return BrotliResult::ResultSuccess;
               }
            }
        }
    }
}
