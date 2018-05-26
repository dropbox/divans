use core;
use interface::{DivansResult, ErrMsg, DivansOpResult, StreamMuxer, StreamDemuxer};
use super::interface::ContextMapType;
use super::priors::{PredictionModePriorType};
use alloc::{Allocator, SliceWrapper, SliceWrapperMut};
use alloc_util::{RepurposingAlloc, AllocatedMemoryPrefix, UninitializedOnAlloc};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
};
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    LiteralPredictionModeNibble,
    PredictionModeContextMap,
    u8_to_speed,
    MAX_LITERAL_CONTEXT_MAP_SIZE,
    MAX_ADV_LITERAL_CONTEXT_MAP_SIZE,
    MAX_PREDMODE_SPEED_AND_DISTANCE_CONTEXT_MAP_SIZE,
    NUM_MIXING_VALUES,
};
use ::priors::PriorCollection;
use ::probability::{Speed, CDF16, SpeedPalette};



pub struct PredictionModeState<AllocU8:Allocator<u8>> {
    pub pm:PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>,
    pub state: PredictionModeSubstate,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PredictionModeSubstate {
    Begin,
    DynamicContextMixing,
    PriorDepth(bool),
    AdaptationSpeed(u32, [(u8,u8);4], bool),
    ContextMapMnemonic(u32, ContextMapType, bool),
    ContextMapFirstNibble(u32, ContextMapType, bool),
    ContextMapSecondNibble(u32, ContextMapType, u8, bool),
    MixingValues(usize, bool),
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
impl <AllocU8:Allocator<u8>> PredictionModeState<AllocU8> {
    pub fn begin(m8:&mut RepurposingAlloc<u8, AllocU8>) -> Self {
        let mut ret = Self::nop();
        ret.free(m8);
        ret.reset(m8);
        ret
    }
    pub fn free(&mut self, m8:&mut RepurposingAlloc<u8, AllocU8>) {
        m8.use_cached_allocation::<UninitializedOnAlloc>().free_cell(
            core::mem::replace(&mut self.pm.literal_context_map,
                               AllocatedMemoryPrefix::<u8, AllocU8>::default()));
                    
        m8.use_cached_allocation::<UninitializedOnAlloc>().free_cell(
            core::mem::replace(&mut self.pm.predmode_speed_and_distance_context_map,
                               AllocatedMemoryPrefix::<u8, AllocU8>::default()));
    }
    pub fn reset(&mut self, m8:&mut RepurposingAlloc<u8, AllocU8>) {
        if self.pm.literal_context_map.0.slice().len() == 0 {
            let lit = m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(MAX_ADV_LITERAL_CONTEXT_MAP_SIZE);
            self.pm = PredictionModeContextMap::<AllocatedMemoryPrefix<u8, AllocU8>> {
                literal_context_map:lit,
                predmode_speed_and_distance_context_map:m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(
                    MAX_PREDMODE_SPEED_AND_DISTANCE_CONTEXT_MAP_SIZE),
            };
        }
        self.state = PredictionModeSubstate::Begin;
    }
    pub fn nop() -> Self {
        PredictionModeState::<AllocU8> {
            pm:PredictionModeContextMap::<AllocatedMemoryPrefix<u8, AllocU8>> {
                literal_context_map:AllocatedMemoryPrefix::<u8, AllocU8>::default(),
                predmode_speed_and_distance_context_map:AllocatedMemoryPrefix::<u8, AllocU8>::default(),
            },
            state:PredictionModeSubstate::Begin,
        }
    }
    #[cfg_attr(not(feature="no-inline"), inline(always))]
    pub fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                            Specialization:EncoderOrDecoderSpecialization,
                            LinearInputBytes:StreamDemuxer<AllocU8>,
                             LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                             Cdf16:CDF16,
                        AllocCDF16:Allocator<Cdf16>,
                        SliceType:SliceWrapper<u8>+Default>(&mut self,
                                               superstate: &mut CrossCommandState<ArithmeticCoder,
                                                                                  Specialization,
                                                                                  LinearInputBytes,
                                                                                  LinearOutputBytes,
                                                                                  Cdf16,
                                                                                  AllocU8,
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
            match superstate.drain_or_fill_internal_buffer_cmd(output_bytes, output_offset) {
                DivansResult::Success => {},
                need_something => return need_something,
            }
            let billing = BillingDesignation::PredModeCtxMap(match self.state {
                PredictionModeSubstate::ContextMapMnemonic(
                    _, context_map_type, _) => PredictionModeSubstate::ContextMapMnemonic(0,
                                                                                    context_map_type, true),
                PredictionModeSubstate::ContextMapFirstNibble(
                    _, context_map_type, _) => PredictionModeSubstate::ContextMapFirstNibble(0,
                                                                                       context_map_type, true),
                PredictionModeSubstate::ContextMapSecondNibble(
                    _, context_map_type, _, _) => PredictionModeSubstate::ContextMapSecondNibble(0,
                                                                                           context_map_type,
                                                                                           0, true),
                PredictionModeSubstate::AdaptationSpeed(_,_, _) => PredictionModeSubstate::FullyDecoded,
                PredictionModeSubstate::MixingValues(_, _) => PredictionModeSubstate::MixingValues(0, true),
                a => a,
            });

            match self.state {
               PredictionModeSubstate::Begin => {
                   superstate.bk.reset_context_map_lru();
                   superstate.bk.reset_distance_context_map();
                   let mut beg_nib = in_cmd.literal_prediction_mode().prediction_mode();
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::Only, (0,));
                       superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                       nibble_prob.blend(beg_nib, Speed::MED);
                   }
                   let pred_mode = match LiteralPredictionModeNibble::new(beg_nib) {
                      Err(x) => return DivansResult::Failure(ErrMsg::PredictionModeFail(x)),
                      Ok(pred_mode) => pred_mode,
                   };
                   self.pm.set_literal_prediction_mode(pred_mode);
                   self.state = PredictionModeSubstate::DynamicContextMixing;
               },
               PredictionModeSubstate::DynamicContextMixing => {
                   let is_adv = in_cmd.get_is_adv_context_map();
                   if (is_adv >> 1) != 0 {
                       return return DivansResult::Failure(ErrMsg::AdvContextMapNotBoolean(is_adv));
                   }
                   assert_eq!(superstate.bk.desired_context_mixing >>3, 0);
                   let mut beg_nib = superstate.bk.desired_context_mixing | (is_adv << 3);
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(
                           PredictionModePriorType::DynamicContextMixingSpeed, (0,));
                       superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                       nibble_prob.blend(beg_nib, Speed::MED);
                   }
                   self.pm.set_mixing_math(beg_nib & 3);
                   self.pm.set_adv_context_map(beg_nib >> 2);
                   //FIXME: carry this in the PredictionMode
                   //superstate.bk.obs_dynamic_context_mixing(beg_nib, &mut superstate.mcdf16);
                   self.state = PredictionModeSubstate::PriorDepth(beg_nib != 0);
               },
               PredictionModeSubstate::PriorDepth(combine_literal_predictions) => {
                   let mut beg_nib = superstate.bk.desired_prior_depth;
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(
                           PredictionModePriorType::PriorDepth, (0,));
                       superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                       nibble_prob.blend(beg_nib, Speed::FAST);
                   }
                   superstate.bk.obs_prior_depth(beg_nib); // FIXME: this is not persisted in the command
                   self.state = PredictionModeSubstate::AdaptationSpeed(0, [(0,0);4], combine_literal_predictions);
               }
               PredictionModeSubstate::AdaptationSpeed(index, mut out_adapt_speed, combine_literal_predictions) => {
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
                   superstate.coder.get_or_put_nibble(&mut nibble, nibble_prob, billing);
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
                       self.pm.set_stride_context_speed([(u8_to_speed(out_adapt_speed[0].0),u8_to_speed(out_adapt_speed[0].1)),
                                                         (u8_to_speed(out_adapt_speed[1].0),u8_to_speed(out_adapt_speed[1].1))]);
                       self.pm.set_context_map_speed([(u8_to_speed(out_adapt_speed[2].0),u8_to_speed(out_adapt_speed[2].1)),
                                                      (u8_to_speed(out_adapt_speed[3].0),u8_to_speed(out_adapt_speed[3].1))]);
                           
                       self.state = PredictionModeSubstate::ContextMapMnemonic(0, ContextMapType::Literal, combine_literal_predictions);
                   } else {
                       self.state = PredictionModeSubstate::AdaptationSpeed(index + 1, out_adapt_speed, combine_literal_predictions);
                   }
               },
               PredictionModeSubstate::ContextMapMnemonic(index, context_map_type, combine_literal_predictions) => {
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
                       superstate.coder.get_or_put_nibble(&mut mnemonic_nibble, nibble_prob, billing);
                       nibble_prob.blend(mnemonic_nibble, Speed::MED);
                   }
                   if mnemonic_nibble == 14 {
                       match context_map_type {
                           ContextMapType::Literal => { // switch to distance context map
                               if self.pm.get_is_adv_context_map() == 0 {
                                   let (src, dst) = self.pm.literal_context_map.slice_mut().split_at_mut(MAX_LITERAL_CONTEXT_MAP_SIZE);
                                   let mut dst_offset = 0;
                                   while dst.len() != dst_offset {
                                        let amt_to_copy = core::cmp::min(src.len(), dst.len() - dst_offset);
                                        let (target, new_dst) = dst.split_at_mut(dst_offset).1.split_at_mut(amt_to_copy);
                                        target.clone_from_slice(src.split_at(amt_to_copy).0);
                                        dst_offset += amt_to_copy;
                                   }
                               }
                               superstate.bk.reset_context_map_lru(); // distance context map should start with 0..14 as lru
                               self.state = PredictionModeSubstate::ContextMapMnemonic(0, ContextMapType::Distance, combine_literal_predictions);
                           },
                           ContextMapType::Distance => { // finished
                               self.state = PredictionModeSubstate::MixingValues(0, combine_literal_predictions);
                           }
                       }
                   } else if mnemonic_nibble == 15 {
                       self.state = PredictionModeSubstate::ContextMapFirstNibble(index, context_map_type, combine_literal_predictions);
                   } else {
                       let val = if mnemonic_nibble == 13 {
                           superstate.bk.cmap_lru.iter().max().unwrap().wrapping_add(1)
                       } else {
                           superstate.bk.cmap_lru[mnemonic_nibble as usize]
                       };
                       if let DivansOpResult::Failure(m) = superstate.bk.obs_context_map_for_lru(context_map_type,
                                                                                                 index,
                                                                                                 val) {
                           return DivansResult::Failure(m);
                       }
                       let mut out_context_map = match context_map_type {
                           ContextMapType::Literal => self.pm.literal_context_map.slice_mut(),
                           ContextMapType::Distance => if self.pm.has_context_speeds() {self.pm.distance_context_map_mut() } else {&mut[]},
                       };
                       if (index as usize) < out_context_map.len() {
                           out_context_map[index as usize] = val;
                       } else {
                           return DivansResult::Failure(ErrMsg::IndexBeyondContextMapSize(index as u8, (index >> 8) as u8));
                       }
                       self.state = PredictionModeSubstate::ContextMapMnemonic(index + 1, context_map_type, combine_literal_predictions);
                   }
               },
               PredictionModeSubstate::ContextMapFirstNibble(index, context_map_type, combine_literal_predictions) => {
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

                   superstate.coder.get_or_put_nibble(&mut msn_nib, nibble_prob, billing);
                   nibble_prob.blend(msn_nib, Speed::MED);
                   self.state = PredictionModeSubstate::ContextMapSecondNibble(index, context_map_type, msn_nib, combine_literal_predictions);
               },
               PredictionModeSubstate::ContextMapSecondNibble(index, context_map_type, most_significant_nibble, combine_literal_predictions) => {
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
                       superstate.coder.get_or_put_nibble(&mut lsn_nib, nibble_prob, billing);
                       nibble_prob.blend(lsn_nib, Speed::MED);
                   }
                   let mut out_context_map = match context_map_type {
                       ContextMapType::Literal => self.pm.literal_context_map.slice_mut(),
                       ContextMapType::Distance => if self.pm.has_context_speeds() {self.pm.distance_context_map_mut() } else {&mut[]},
                   };
                   if (index as usize) < out_context_map.len() {
                       out_context_map[index as usize] = (most_significant_nibble << 4) | lsn_nib;
                   } else {
                       return DivansResult::Failure(ErrMsg::IndexBeyondContextMapSize(index as u8, (index >> 8) as u8));
                   }
                   if let DivansOpResult::Failure(m) = superstate.bk.obs_context_map_for_lru(context_map_type, index, (most_significant_nibble << 4) | lsn_nib) {
                       return DivansResult::Failure(m);
                   }
                   self.state = PredictionModeSubstate::ContextMapMnemonic(index + 1, context_map_type, combine_literal_predictions);
               },
               PredictionModeSubstate::MixingValues(index, combine_literal_predictions) => {
                   let mut mixing_nib = if !superstate.bk.materialized_context_map {
                       4
                   } else if !combine_literal_predictions {
                       0
                   } else if in_cmd.has_context_speeds() {
                       in_cmd.get_mixing_values()[index]
                   } else {
                       0
                   };
                       
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(
                           PredictionModePriorType::PriorMixingValue, (0,));
                       superstate.coder.get_or_put_nibble(&mut mixing_nib, nibble_prob, billing);
                       nibble_prob.blend(mixing_nib, Speed::PLANE);
                   }
                   self.pm.get_mixing_values_mut()[index] = mixing_nib;
                   if index + 1 == NUM_MIXING_VALUES {
                       // reconsil
                       self.state = PredictionModeSubstate::FullyDecoded;
                   } else {
                       /* FIXME: this should be done in obs_prediction_mode_context_map in LiteralBookKeeping
                       match superstate.bk.obs_mixing_value(index, mixing_nib) {
                           DivansOpResult::Success => {
                               
                           },
                           DivansOpResult::Failure(m) => return DivansResult::Failure(m),
                   }*/
                       self.state = PredictionModeSubstate::MixingValues(index + 1, combine_literal_predictions);
                   }
               },
               PredictionModeSubstate::FullyDecoded => {
                   return DivansResult::Success;
               }
            }
        }
    }
}
