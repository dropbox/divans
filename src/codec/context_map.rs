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
type SpeedPalette = [(u8,u8); 15];
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PredictionModeState {
    Begin,
    DynamicContextMixing,
    PriorDepth,
    LiteralAdaptationRate,
    ContextMapMnemonic(u32, ContextMapType),
    ContextMapFirstNibble(u32, ContextMapType),
    ContextMapSecondNibble(u32, ContextMapType, u8),
    ContextMapSpeedPalette(u32, SpeedPalette),
    ContextMapSpeeds(u32, SpeedPalette, u8),
    FullyDecoded,
}

fn build_palette(data:&[u8]) -> Option<[(u8, u8);15]> {
    let mut ret = [(255u8,255u8);15];
    let mut cached = 0usize;
    for outer in 0.. data.len()/1024 {
        for inner in 0..512 {
            let inc = data[outer * 1024 + inner];
            let max = data[outer * 1024 + inner + 512];
            let mut found = false;
            for item in &ret[..cached] {
                if *item == (inc, max) {
                    found = true;
                    break;
                }
            }
            if !found {
                if cached == ret.len() {
                    unimplemented!();
                    return None;
                }
                ret[cached] = (inc, max);
                cached += 1;
            }
        }
    }
    ret.sort_unstable();
    return Some(ret); //fixme
}

fn lookup_palette(palette: &SpeedPalette, inc: u8, max:u8) -> Option<u8> {
    for (i, val) in palette.iter().enumerate() {
        if val.0 == inc && val.1 == max {
            return Some(i as u8 & 0xf)
        }
    }
    return None
}

fn next_mul_512(a: usize) -> usize {
    a + (512 - (a & 511))
}

fn all_same(data_a: &[u8], data_b: &[u8], goal: (u8, u8) ) -> bool {
    assert!(data_a.len() == data_b.len());
    if data_a.len() == 0 {
        return true;
    }
    if data_a[data_a.len() - 1] != goal.0 {
        return false;
    }
    if data_b[data_b.len() - 1] != goal.1 {
        return false;
    }
    for i in data_a.iter().zip(data_b.iter()) {
        if *i.0 != goal.0 || *i.1 != goal.1 {
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
        let max_speed_index = 1024;// 2048; // can't get stride working yet
        let mut speed_palette: Option<[(u8,u8);15]> = None;
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
                PredictionModeState::ContextMapSpeeds(
                    _, _, _) => PredictionModeState::FullyDecoded,
                PredictionModeState::ContextMapSpeedPalette(
                    _, _) => PredictionModeState::FullyDecoded,
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
                               *self = PredictionModeState::ContextMapSpeedPalette(0, [(0,0);15]);
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
               PredictionModeState::ContextMapSpeedPalette(index, mut out_palette) => {
                   if speed_palette.is_none() {
                       speed_palette = build_palette(in_cmd.context_speeds.slice());
                   }
                   let palette_index = index as usize >> 2;
                   let cur_speed = speed_palette.unwrap()[palette_index];
                   let prev_speed = if palette_index != 0 {
                       speed_palette.unwrap()[palette_index - 1]
                   } else {
                       (0, 0)
                   };
                   let palette_type = index & 3;
                   let mut nibble: u8;
                   /*if cur_speed == (0, 0) {
                       nibble = 0xf
                   } else */if palette_type == 0 {
                       nibble = (cur_speed.0.wrapping_sub(prev_speed.0)&0x7f) >> 3;
                   } else if palette_type == 1 {
                       nibble = (cur_speed.0.wrapping_sub(prev_speed.0)&0x7f) & 0x7;
                   } else if palette_type == 2 {
                       nibble = (cur_speed.1.wrapping_sub(prev_speed.1)&0x7f) >> 3;
                   } else {
                       nibble = (cur_speed.1.wrapping_sub(prev_speed.1)&0x7f) & 0x7;
                   }
                   let mut nibble_prob = superstate.bk.prediction_priors.get(PredictionModePriorType::ContextMapSpeedPalette,
                                                                             (palette_type as usize,));
                   superstate.coder.get_or_put_nibble(&mut nibble, nibble_prob, billing);
                   //print!("{} {} ({} {}) Putting {}\n", palette_index, palette_type, cur_speed.0, cur_speed.1, nibble);
                   nibble_prob.blend(nibble, Speed::SLOW);
                   if false && nibble == 0xf {
                       *self = PredictionModeState::ContextMapSpeeds(0, out_palette, 0);
                   } else {
                       if palette_type == 0 {
                           out_palette[palette_index].0 |= nibble<<3;
                       }
                       if palette_type == 1 {
                           out_palette[palette_index].0 |= nibble;
                       }
                       if palette_type == 2 {
                           out_palette[palette_index].1 |= nibble << 3;
                       }
                       if palette_type == 3 {
                           out_palette[palette_index].1 |= nibble;
                           if palette_index != 0 {
                               let prev_palette = out_palette[palette_index - 1];
                               let cur_palette  = out_palette[palette_index];
                               out_palette[palette_index].0 = cur_palette.0.wrapping_add(prev_palette.0) & 0x7f;
                               out_palette[palette_index].1 = cur_palette.1.wrapping_add(prev_palette.1) & 0x7f;
                           }
                       }
                       if index as usize + 1 == 4 * out_palette.len(){
                           *self = PredictionModeState::ContextMapSpeeds(0, out_palette, 0);
                       } else {
                           *self = PredictionModeState::ContextMapSpeedPalette(index + 1, out_palette);
                       }
                   }
               },
               PredictionModeState::ContextMapSpeeds(mut input_index, palette, last_val) => {
                    let speed_index_offset = 1024;
                    let speed_index_mask = if superstate.bk.combine_literal_predictions {
                       0xffff
                   } else {
                       0x7ff // so we wrap around and pick up vanilla stride instead of the combination
                   };
                   let speeds = in_cmd.context_speeds.slice();
                   let speed_lookup_index = ((input_index + speed_index_offset) & speed_index_mask) as usize;
                   let mut nibble = if input_index as usize + 512 >= speeds.len() || all_same(
                       &speeds[speed_lookup_index..next_mul_512(speed_lookup_index)],
                       &speeds[speed_lookup_index+512..next_mul_512(speed_lookup_index+512)],
                       palette[last_val as usize]) {
                       0xf
                   } else {
                       lookup_palette(&palette, speeds[speed_lookup_index], speeds[speed_lookup_index + 512]).unwrap()
                   };
                   {
                       let mut nibble_prob = superstate.bk.prediction_priors.get(
                           PredictionModePriorType::ContextMapSpeeds,
                           (0,));
                       superstate.coder.get_or_put_nibble(&mut nibble, nibble_prob, billing);
                       nibble_prob.blend(nibble, Speed::SLOW);
                   }
                   if nibble == 0xf {
                       nibble = last_val;
                       for i in input_index as usize..next_mul_512(input_index as usize) {
                           superstate.bk.obs_literal_speed(i as u32, in_cmd, palette[last_val as usize]);
                       }
                       input_index = next_mul_512(input_index as usize) as u32;
                   } else {
                       superstate.bk.obs_literal_speed(input_index, in_cmd, palette[nibble as usize]);
                       input_index += 1;
                   }
                   if (input_index & 1023) == 512 {
                       input_index += 512;
                   }
                   if input_index == max_speed_index {
                       *self = PredictionModeState::FullyDecoded;
                   } else {
                       *self = PredictionModeState::ContextMapSpeeds(input_index, palette, nibble);
                   }
               },
                PredictionModeState::FullyDecoded => {
                    /*
                    for stride in 0..2 {
                        for prior in 0..256 {
                            for high in 0..2 {
                                println!("({}, {}, {}: {:?}",
                                         stride,prior,high,
                                         superstate.bk.context_speed[stride][prior][high]);
                            }
                        }
                    }*/
                   return BrotliResult::ResultSuccess;
               }
            }
        }
    }
}
