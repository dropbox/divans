use interface::{DivansResult, ErrMsg, DivansOpResult, StreamMuxer, StreamDemuxer};
use super::interface::ContextMapType;
use super::priors::{PredictionModePriorType};
use alloc::{Allocator, SliceWrapper};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    CMD_CODER,
};
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    LiteralPredictionModeNibble,
    PredictionModeContextMap,
};
use ::priors::PriorCollection;
use ::probability::{Speed, CDF2, CDF16, SpeedPalette};
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PredictionModeState {
    Begin,
    DynamicContextMixing,
    PriorDepth,
    MixingValues(usize),
    AdaptationSpeed(u32, [(u8,u8);4]),
    ContextMapMnemonic(u32, ContextMapType),
    ContextMapFirstNibble(u32, ContextMapType),
    ContextMapSecondNibble(u32, ContextMapType, u8),
    FullyDecoded,
}


//returns if a is closer than b
fn closer(candidate: i16, best: i16, item: i16) -> bool {
    let mut cand_dist = i32::from(candidate) - i32::from(item);
    let mut best_dist = i32::from(best) - i32::from(item);
    if best_dist < 0 {
        best_dist = -best_dist;
    }
    if cand_dist < 0 {
        cand_dist = -cand_dist;
    }
    best_dist > cand_dist
}
fn find_best_match(data: Speed, palette: &SpeedPalette) -> usize {
    let mut best_match = palette[0];
    let mut best_index = 0;
    for (index, item) in palette.iter().enumerate() {
        if closer(item.inc(), best_match.inc(), data.inc()) || (item.inc() == best_match.inc() && closer(item.lim(), best_match.lim(), data.lim())) {
            best_match = *item;
            best_index = index;
        }
    }
    best_index
}
impl PredictionModeState {
    pub fn begin() -> Self {
        PredictionModeState::Begin
    }
    #[cfg_attr(not(feature="no-inline"), inline(always))]
    pub fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                            Specialization:EncoderOrDecoderSpecialization,
                            LinearInputBytes:StreamDemuxer<AllocU8>+Default,
                             LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                             Cdf16:CDF16,
                        AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<CDF2>,
                        AllocCDF16:Allocator<Cdf16>,
                        SliceType:SliceWrapper<u8>+Default>(&mut self,
                                               superstate: &mut CrossCommandState<ArithmeticCoder,
                                                                                  Specialization,
                                                                                  LinearInputBytes,
                                                                                  LinearOutputBytes,
                                                                                  Cdf16,
                                                                                  AllocU8,
                                                                                  AllocCDF2,
                                                                                  AllocCDF16>,
                                               in_cmd: &PredictionModeContextMap<SliceType>,
                                               output_bytes:&mut [u8],
                                               output_offset: &mut usize) -> DivansResult {
        let mut desired_speeds = [super::interface::default_literal_speed();4];
        if in_cmd.has_context_speeds() {
            let cm = in_cmd.context_map_speed_f8();
            if cm[0].0 != 0 || cm[0].1 != 0 {
                desired_speeds[2] = Speed::from_f8_tuple(cm[0]);
            }
            if cm[1].0 != 0 || cm[1].1 != 0 {
                desired_speeds[3] = Speed::from_f8_tuple(cm[1]);
            }
            let stride;
            if superstate.bk.desired_context_mixing != 0 {
                stride = in_cmd.combined_stride_context_speed_f8();
            } else {
                stride = in_cmd.stride_context_speed_f8();
            }
            if stride[0].0 != 0 || stride[0].1 != 0 {
                desired_speeds[0] = Speed::from_f8_tuple(stride[0]);
            }
            if stride[1].0 != 0 || stride[1].1 != 0 {
                desired_speeds[1] = Speed::from_f8_tuple(stride[1]);
            }
        }
        if let Some(adapt) = superstate.bk.desired_literal_adaptation {
            desired_speeds = adapt;
        }
        loop {
            match superstate.drain_or_fill_internal_buffer(CMD_CODER, output_bytes, output_offset) {
                DivansResult::Success => {},
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
                PredictionModeState::AdaptationSpeed(_,_) => PredictionModeState::FullyDecoded,
                PredictionModeState::MixingValues(_) => PredictionModeState::MixingValues(0),
                a => a,
            });

            match *self {
               PredictionModeState::Begin => {
                   superstate.bk.reset_context_map_lru();
                   superstate.bk.reset_context_map();
                   let mut beg_nib = in_cmd.literal_prediction_mode().prediction_mode();
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::Only, (0,));
                       superstate.coder[CMD_CODER].get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                       nibble_prob.blend(beg_nib, Speed::MED);
                   }
                   let pred_mode = match LiteralPredictionModeNibble::new(beg_nib) {
                      Err(x) => return DivansResult::Failure(ErrMsg::PredictionModeFail(x)),
                      Ok(pred_mode) => pred_mode,
                   };
                   match superstate.bk.obs_pred_mode(pred_mode) {
                       DivansOpResult::Failure(m) => return DivansResult::Failure(m),
                       DivansOpResult::Success => {},
                   }
                   *self = PredictionModeState::DynamicContextMixing;
               },
               PredictionModeState::DynamicContextMixing => {
                   let mut beg_nib = superstate.bk.desired_context_mixing;
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(
                           PredictionModePriorType::DynamicContextMixingSpeed, (0,));
                       superstate.coder[CMD_CODER].get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                       nibble_prob.blend(beg_nib, Speed::MED);
                   }
                   superstate.bk.obs_dynamic_context_mixing(beg_nib, &mut superstate.mcdf16);
                   *self = PredictionModeState::PriorDepth;
               },
               PredictionModeState::PriorDepth => {
                   let mut beg_nib = superstate.bk.desired_prior_depth;
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(
                           PredictionModePriorType::PriorDepth, (0,));
                       superstate.coder[CMD_CODER].get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                       nibble_prob.blend(beg_nib, Speed::FAST);
                   }
                   superstate.bk.obs_prior_depth(beg_nib);
                   superstate.bk.clear_mixing_values();
                   *self = PredictionModeState::AdaptationSpeed(0, [(0,0);4]);
               }
               PredictionModeState::AdaptationSpeed(index, mut out_adapt_speed) => {
                   let speed_index = index as usize >> 2;
                   let cur_speed = desired_speeds[speed_index].to_f8_tuple();
                   let palette_type = index & 3;
                   let mut nibble: u8;
                   if palette_type == 0 {
                       nibble = (cur_speed.0&0x7f) >> 3;
                   } else if palette_type == 1 {
                       nibble = (cur_speed.0&0x7f) & 0x7;
                   } else if palette_type == 2 {
                       nibble = (cur_speed.1&0x7f) >> 3;
                   } else {
                       nibble = (cur_speed.1&0x7f) & 0x7;
                   }
                   let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::ContextMapSpeedPalette,
                                                                             (palette_type as usize,));
                   superstate.coder[CMD_CODER].get_or_put_nibble(&mut nibble, nibble_prob, billing);
                   nibble_prob.blend(nibble, Speed::FAST);
                   if palette_type == 0 {
                       out_adapt_speed[speed_index].0 |= nibble<<3;
                   }
                   if palette_type == 1 {
                       out_adapt_speed[speed_index].0 |= nibble;
                   }
                   if palette_type == 2 {
                       out_adapt_speed[speed_index].1 |= nibble << 3;
                   }
                   if palette_type == 3 {
                       out_adapt_speed[speed_index].1 |= nibble;
                   }
                   if index as usize + 1 == 4 * out_adapt_speed.len(){
                       let mut tmp = [Speed::MUD; 4];
                       for (out_item, in_item) in tmp.iter_mut().zip(out_adapt_speed.iter()) {
                           *out_item = Speed::from_f8_tuple(*in_item);
                       }
                       superstate.bk.literal_adaptation = tmp;
                       *self = PredictionModeState::ContextMapMnemonic(0, ContextMapType::Literal);
                   } else {
                       *self = PredictionModeState::AdaptationSpeed(index + 1, out_adapt_speed);
                   }
               },
               PredictionModeState::ContextMapMnemonic(index, context_map_type) => {
                   let mut cur_context_map = match context_map_type {
                           ContextMapType::Literal => in_cmd.literal_context_map.slice(),
                           ContextMapType::Distance => if in_cmd.has_context_speeds() {in_cmd.distance_context_map() } else {&[]},
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
                       superstate.coder[CMD_CODER].get_or_put_nibble(&mut mnemonic_nibble, nibble_prob, billing);
                       nibble_prob.blend(mnemonic_nibble, Speed::MED);
                   }
                   if mnemonic_nibble == 14 {
                       match context_map_type {
                           ContextMapType::Literal => { // switch to distance context map
                               superstate.bk.reset_context_map_lru(); // distance context map should start with 0..14 as lru
                               *self = PredictionModeState::ContextMapMnemonic(0, ContextMapType::Distance);
                           },
                           ContextMapType::Distance => { // finished
                               *self = PredictionModeState::MixingValues(0);
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
                       if let DivansOpResult::Failure(m) = superstate.bk.obs_context_map(context_map_type, index, val) {
                           return DivansResult::Failure(m);
                       }
                       *self = PredictionModeState::ContextMapMnemonic(index + 1, context_map_type);
                   }
               },
               PredictionModeState::ContextMapFirstNibble(index, context_map_type) => {
                   let cur_context_map = match context_map_type {
                       ContextMapType::Literal => in_cmd.literal_context_map.slice(),
                       ContextMapType::Distance => if in_cmd.has_context_speeds() {in_cmd.distance_context_map() } else {&[]},
                   };
                   let mut msn_nib = if index as usize >= cur_context_map.len() {
                       // encode nothing
                       0
                   } else {
                       cur_context_map[index as usize] >> 4
                   };
                   let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::FirstNibble, (0,));

                   superstate.coder[CMD_CODER].get_or_put_nibble(&mut msn_nib, nibble_prob, billing);
                   nibble_prob.blend(msn_nib, Speed::MED);
                   *self = PredictionModeState::ContextMapSecondNibble(index, context_map_type, msn_nib);
               },
               PredictionModeState::ContextMapSecondNibble(index, context_map_type, most_significant_nibble) => {
                   let cur_context_map = match context_map_type {
                       ContextMapType::Literal => in_cmd.literal_context_map.slice(),
                       ContextMapType::Distance => if in_cmd.has_context_speeds() {in_cmd.distance_context_map() } else {&[]},
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
                       superstate.coder[CMD_CODER].get_or_put_nibble(&mut lsn_nib, nibble_prob, billing);
                       nibble_prob.blend(lsn_nib, Speed::MED);
                   }
                   if let DivansOpResult::Failure(m) = superstate.bk.obs_context_map(context_map_type, index, (most_significant_nibble << 4) | lsn_nib) {
                       return DivansResult::Failure(m);
                   }
                   *self = PredictionModeState::ContextMapMnemonic(index + 1, context_map_type);
               },
               PredictionModeState::MixingValues(index) => {
                   let mut mixing_nib = if !superstate.bk.materialized_context_map {
                       4
                   } else if !superstate.bk.combine_literal_predictions {
                       0
                   } else if in_cmd.has_context_speeds() {
                       in_cmd.get_mixing_values()[index]
                   } else {
                       0
                   };
                       
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(
                           PredictionModePriorType::PriorMixingValue, (0,));
                       superstate.coder[CMD_CODER].get_or_put_nibble(&mut mixing_nib, nibble_prob, billing);
                       nibble_prob.blend(mixing_nib, Speed::PLANE);
                   }
                   if index + 1 == superstate.bk.mixing_mask.len() {
                       // reconsil
                       *self = PredictionModeState::FullyDecoded;
                   } else {
                       match superstate.bk.obs_mixing_value(index, mixing_nib) {
                           DivansOpResult::Success => {
                               *self = PredictionModeState::MixingValues(index + 1);
                           },
                           DivansOpResult::Failure(m) => return DivansResult::Failure(m),
                       }
                   }
               },
               PredictionModeState::FullyDecoded => {
                   return DivansResult::Success;
               }
            }
        }
    }
}
