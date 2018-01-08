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
    LiteralAdaptationRate,
    ContextMapMnemonic(u32, ContextMapType),
    ContextMapFirstNibble(u32, ContextMapType),
    ContextMapSecondNibble(u32, ContextMapType, u8),
    FullyDecoded,
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
                       let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::Mnemonic, (0,));
                       superstate.coder.get_or_put_nibble(&mut mnemonic_nibble, nibble_prob, billing);
                       nibble_prob.blend(mnemonic_nibble, Speed::MED);
                   }
                   if mnemonic_nibble == 14 {
                       match context_map_type {
                           ContextMapType::Literal => { // switch to distance context map
                               superstate.bk.reset_context_map_lru(); // distance context map should start with 0..14 as lru
                               *self = PredictionModeState::ContextMapMnemonic(0, ContextMapType::Distance);
                           },
                           ContextMapType::Distance => { // finished
                               *self = PredictionModeState::FullyDecoded;
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
                   let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::FirstNibble, (0,));

                   superstate.coder.get_or_put_nibble(&mut msn_nib, nibble_prob, billing);
                   nibble_prob.blend(msn_nib, Speed::MED);
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
                       let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::SecondNibble, (0,));
                       // could put first_nibble as ctx instead of 0, but that's probably not a good idea since we never see
                       // the same nibble twice in all likelihood if it was covered by the mnemonic--unless we want random (possible?)
                       superstate.coder.get_or_put_nibble(&mut lsn_nib, nibble_prob, billing);
                       nibble_prob.blend(lsn_nib, Speed::MED);
                   }
                   if let BrotliResult::ResultFailure = superstate.bk.obs_context_map(context_map_type, index, (most_significant_nibble << 4) | lsn_nib) {
                       return BrotliResult::ResultFailure;
                   }
                   *self = PredictionModeState::ContextMapMnemonic(index + 1, context_map_type);
               },
               PredictionModeState::FullyDecoded => {
                   if in_cmd.nibble_pdf.slice().len() != 0 { // FIXME: this must be the decoded, not encoded, state
                     return superstate.bk.obs_literal_pdfs(in_cmd.nibble_pdf.slice());
                   } else {
                     return BrotliResult::ResultSuccess;
                   }
               }
            }
        }
    }
}
