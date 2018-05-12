use core;
use interface::{DivansResult, StreamMuxer, StreamDemuxer};
use ::probability::{CDF16, Speed, ExternalProbCDF16};
use super::priors::{LiteralNibblePriorType, LiteralCommandPriorType, LiteralCMPriorType};
use ::slice_util::AllocatedMemoryPrefix;
use ::alloc_util::UninitializedOnAlloc;
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    ByteContext,
    round_up_mod_4,
    LiteralBookKeeping,
    LIT_CODER,
    CMD_CODER,
    drain_or_fill_static_buffer,
    ThreadContext,
};

use super::specializations::{CodecTraits};
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    LiteralCommand,
};
use super::priors::LiteralNibblePriors;
use ::priors::PriorCollection;
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LiteralSubstate {
    Begin,
    LiteralCountSmall(bool),
    LiteralCountFirst,
    LiteralCountLengthGreater14Less25,
    LiteralCountMantissaNibbles(u8, u32),
    LiteralNibbleIndex(u32),
    SafeLiteralNibbleIndex(u32),
    LiteralNibbleLowerHalf(u32),
    LiteralNibbleIndexWithECDF(u32),
    FullyDecoded,
}

macro_rules! unwrap_ref {
    ($x: expr) => (match $x { Some(ref mut y) => y, None => unreachable!()});
}

const NUM_LITERAL_LENGTH_MNEMONIC: u32 = 14;
pub struct LiteralState<AllocU8:Allocator<u8>> {
    pub lc:LiteralCommand<AllocatedMemoryPrefix<u8, AllocU8>>,
    pub state: LiteralSubstate,
}


trait NibbleArrayCallSite {
   const FULLY_SAFE: bool;
   const SECOND_HALF: bool;
}
struct NibbleArraySafe {}
impl NibbleArrayCallSite for NibbleArraySafe {
   const FULLY_SAFE: bool = true;
   const SECOND_HALF: bool = false;
}

struct NibbleArrayLowBuffer {}
impl NibbleArrayCallSite for NibbleArrayLowBuffer {
   const FULLY_SAFE: bool = false;
   const SECOND_HALF: bool = false;
}

struct NibbleArraySecond {}
impl NibbleArrayCallSite for NibbleArraySecond {
   const FULLY_SAFE: bool = false;
   const SECOND_HALF: bool = true;
}

trait HighTrait {
    const IS_HIGH: bool;
}
struct HighNibble{}
struct LowNibble{}
impl HighTrait for HighNibble {
    const IS_HIGH: bool = true;
}
impl HighTrait for LowNibble {
    const IS_HIGH: bool = false;
}
#[inline(always)]
pub fn get_prev_word_context<Cdf16:CDF16,
                             AllocU8:Allocator<u8>,
                             AllocCDF16:Allocator<Cdf16>,
                             CTraits:CodecTraits>(lbk: &LiteralBookKeeping<Cdf16,
                                                                               AllocU8,
                                                                               AllocCDF16>,
                                                  _ctraits: &'static CTraits) -> ByteContext {
    //let local_stride = if CTraits::HAVE_STRIDE { core::cmp::max(1, bk.stride) } else {1};
    //let base_shift = 0x40 - local_stride * 8;
    //let stride_byte = ((bk.last_8_literals >> base_shift) & 0xff) as u8;
    let prev_byte = ((lbk.last_8_literals >> 0x38) & 0xff) as u8;
    let prev_prev_byte = ((lbk.last_8_literals >> 0x30) & 0xff) as u8;
    let selected_context = lbk.literal_lut0[prev_byte as usize] | lbk.literal_lut1[prev_prev_byte as usize];
    /*
    let selected_context = match bk.literal_prediction_mode.0 {
        LITERAL_PREDICTION_MODE_SIGN => (
            constants::SIGNED_3_BIT_CONTEXT_LOOKUP[prev_byte as usize] << 3
        ) | constants::SIGNED_3_BIT_CONTEXT_LOOKUP[prev_prev_byte as usize],
        LITERAL_PREDICTION_MODE_UTF8 =>
            constants::UTF8_CONTEXT_LOOKUP[prev_byte as usize]
            | constants::UTF8_CONTEXT_LOOKUP[prev_prev_byte as usize + 256],
        LITERAL_PREDICTION_MODE_MSB6 => prev_byte >> 2,
        LITERAL_PREDICTION_MODE_LSB6 => prev_byte & 0x3f,
        _ => panic!("Internal Error: parsed nibble prediction mode has more than 2 bits"),
    };
    assert_eq!(selected_context, selected_contextA);
*/
    let cmap_index = selected_context as usize + ((lbk.get_literal_block_type() as usize) << 6);
    let actual_context = lbk.literal_context_map.slice()[cmap_index as usize];
    ByteContext{actual_context:actual_context, stride_bytes:lbk.last_8_literals, prev_byte: prev_byte}
}


impl<AllocU8:Allocator<u8>,
                         > LiteralState<AllocU8> {
    pub fn ecdf_write_nibble<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                        Cdf16:CDF16,
                       >(&mut self,
                         nibble_index: u32,
                         mut cur_nibble: u8,
                         _cur_byte_prior: u8,
                         local_coder: &mut ArithmeticCoder,
                         default_nibble_prob: Cdf16,
                         in_cmd_prob_slice: &[u8]) -> u8 {
        let high_nibble = (nibble_index & 1) == 0;
        let mut ecdf = ExternalProbCDF16::default();
        let shift_offset = if high_nibble { 4usize } else { 0usize };
        let byte_index = (nibble_index as usize) >> 1;
        let en = byte_index*8 + shift_offset + 4;
        if en <= in_cmd_prob_slice.len() {
            let st = en - 4;
            let probs = [in_cmd_prob_slice[st], in_cmd_prob_slice[st + 1],
                             in_cmd_prob_slice[st + 2], in_cmd_prob_slice[st + 3]];
            ecdf.init(cur_nibble, &probs, &default_nibble_prob);
            local_coder.get_or_put_nibble(&mut cur_nibble, &ecdf, BillingDesignation::LiteralCommand(LiteralSubstate::LiteralNibbleIndex(nibble_index & 1)));
        } else {
            local_coder.get_or_put_nibble(&mut cur_nibble, &ecdf, BillingDesignation::LiteralCommand(LiteralSubstate::LiteralNibbleIndex(nibble_index & 1)));
        }
        cur_nibble
    }
    #[cfg_attr(not(feature="no-inline"), inline(always))]
    fn code_nibble<'a, ArithmeticCoder:ArithmeticEncoderOrDecoder,
                   Cdf16:CDF16,
                   AllocCDF16:Allocator<Cdf16>,
                   CTraits:CodecTraits,
                   HTraits:HighTrait,
                   >(&mut self,
                     mut cur_nibble: u8,
                     byte_context: ByteContext,
                     cur_byte_prior: u8,
                     _ctraits: &'static CTraits,
                     _htraits: HTraits,
                     local_coder: &mut ArithmeticCoder,
                     lbk: &mut LiteralBookKeeping<Cdf16,
                                                      AllocU8,
                                                      AllocCDF16>,
                     lit_priors:&'a mut LiteralNibblePriors<Cdf16, AllocCDF16>) -> (u8, Option<&'a mut Cdf16>) {

        // The mixing_mask is a lookup table that determines which priors are most relevant
        // for a particular actual_context. The table is also indexed by the
        // upper half of the current nibble, or the upper half of the previous nibble
        let mut mixing_mask_index = usize::from(byte_context.actual_context);
        if !HTraits::IS_HIGH {
            mixing_mask_index |= usize::from(cur_byte_prior & 0xf) << 8;
            mixing_mask_index |= 4096;
        } else {
            mixing_mask_index |= (usize::from(byte_context.prev_byte) >> 4) << 8;
        }
        let mm_opts = lbk.mixing_mask[mixing_mask_index];

        // if the mixing mask is not zero, the byte, stride distance prior, is a good prior
        let mm = -((mm_opts != 0) as isize) as u8;
        // mix 3 lets us examine just half of the previous byte in addition to the context
        let opt_3_f_mask = ((-((mm_opts == 1) as i8)) & 0xf) as u8; // if mm_opts == 1 {0xf} else {0x0}

        // Choose the stride b based on the mixing mask. The stride offset is 0, 8, 16, 24 or 56 bits
        // this translates into actual strides of 1, 2, 3, 4 or 8 bytes
        let stride_offset = if mm_opts < 4 {0} else {core::cmp::min(7, mm_opts as usize ^ 4) << 3};
        let index_b: usize;
        let index_c: usize;
        // pick the previous byte based on the chosen stride
        let stride_selected_byte = (byte_context.stride_bytes >> (0x38 - stride_offset)) as u8 & 0xff;
        if HTraits::IS_HIGH { // high nibble must depend only on the previous bytes
            index_b = usize::from(stride_selected_byte & mm & (!opt_3_f_mask));
            index_c = usize::from(byte_context.actual_context);
        } else { // low nibble can depend on the upper half of the current byte
            index_b = usize::from((mm & stride_selected_byte) | (!mm & byte_context.actual_context));
            index_c = usize::from(cur_byte_prior | ((byte_context.actual_context & opt_3_f_mask) << 4));
        };
        // select the probability out of a 3x256x256 array of 32 byte nibble-CDFs
        let nibble_prob = lit_priors.get(LiteralNibblePriorType::CombinedNibble,
                                         (usize::from((mm >> 7) ^ (opt_3_f_mask >> 2)),
                                          index_b,
                                          index_c));
        {
            let immutable_prior: Cdf16;
            let coder_prior: &Cdf16;
            if mm_opts == 2 {
                immutable_prior = Cdf16::default();
                coder_prior = &immutable_prior;
            } else {
                coder_prior = nibble_prob;
            }
            if CTraits::MIXING_PRIORS {
                let cm_prob = if HTraits::IS_HIGH {
                    lbk.lit_cm_priors.get(LiteralCMPriorType::FirstNibble,
                                                    (0,//(byte_context.selected_context as i8 & -(bk.prior_depth as i8)) as usize,
                                                     usize::from(byte_context.actual_context),))
                } else {
                    lbk.lit_cm_priors.get(LiteralCMPriorType::SecondNibble,
                                                    (0,//(byte_context.selected_context as i8 & -(bk.prior_depth as i8)) as usize,
                                                     usize::from(cur_byte_prior),
                                                     usize::from(byte_context.actual_context)))
                };
                let prob = cm_prob.average(nibble_prob, lbk.model_weights[HTraits::IS_HIGH as usize].norm_weight() as u16 as i32);
                let weighted_prob_range = local_coder.get_or_put_nibble(
                    &mut cur_nibble,
                    &prob,
                    BillingDesignation::LiteralCommand(LiteralSubstate::LiteralNibbleIndex(!HTraits::IS_HIGH as u32)));
                assert_eq!(lbk.model_weights[HTraits::IS_HIGH as usize].should_mix(), true);
                let model_probs = [
                    cm_prob.sym_to_start_and_freq(cur_nibble).range.freq,
                    nibble_prob.sym_to_start_and_freq(cur_nibble).range.freq,
                ];
                lbk.model_weights[HTraits::IS_HIGH as usize].update(model_probs, weighted_prob_range.freq);
                cm_prob.blend(cur_nibble, lbk.literal_adaptation[2 | HTraits::IS_HIGH as usize].clone());
            } else {
                // actually code (or decode) the byte from the file
                local_coder.get_or_put_nibble(&mut cur_nibble,
                                              coder_prior,
                                              BillingDesignation::LiteralCommand(LiteralSubstate::LiteralNibbleIndex(!HTraits::IS_HIGH as u32)));
            }
        }
        let blendable_prob: Option<&'a mut Cdf16>;
        if mm_opts == 2 {
            blendable_prob = None;
        } else {
            blendable_prob = Some(nibble_prob);
        }
        
        (cur_nibble, blendable_prob)
    }
    //(do not do inline here; doing so causes a sizable perf regression on 1.27.0 nightly 2018-04-18)
    fn code_nibble_array<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                         Specialization:EncoderOrDecoderSpecialization,
                         LinearInputBytes:StreamDemuxer<AllocU8>+Default,
                         LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                         Cdf16:CDF16,
                         AllocCDF16:Allocator<Cdf16>,
                         CTraits:CodecTraits,
                         ISlice: SliceWrapper<u8>,
                         NibbleArrayType: NibbleArrayCallSite,
                         >(&mut self,
                           m8: &mut AllocU8,
                           output_bytes:&mut [u8],
                           output_offset: &mut usize,
                           in_cmd: &LiteralCommand<ISlice>,
                           start_nibble_index: u32,
                           local_coder: &mut ArithmeticCoder,
                           demuxer: &mut LinearInputBytes,
                           muxer: &mut LinearOutputBytes,
                           lit_high_priors:&mut LiteralNibblePriors<Cdf16, AllocCDF16>,
                           lit_low_priors:&mut LiteralNibblePriors<Cdf16, AllocCDF16>,
                           lbk: &mut LiteralBookKeeping<Cdf16,
                                                             AllocU8,
                                                             AllocCDF16>,
                          specialization: &Specialization,
                          _nibble_array_type: NibbleArrayType,
                          ctraits: &'static CTraits,
  ) -> DivansResult {
      let mut lc_data = core::mem::replace(&mut self.lc.data, AllocatedMemoryPrefix::<u8, AllocU8>::default());
      let last_llen = lc_data.slice().len() as u32;
      let start_byte_index = (start_nibble_index as usize) >> 1;
      let mut retval = DivansResult::Success;
      let mut first = true;
      for (byte_offset, lc_target) in lc_data.slice_mut()[start_byte_index..last_llen as usize].iter_mut().enumerate() {
           let mut byte_to_encode_val = specialization.get_literal_byte(in_cmd,
                                                                        start_byte_index.wrapping_add(byte_offset));
           let byte_context = get_prev_word_context(lbk, ctraits);
           let h_nibble;
           let low_buffer_warning;
           if NibbleArrayType::SECOND_HALF == false || first == false {
               let (cur_nibble, cur_prob) = self.code_nibble(byte_to_encode_val >> 4,
                                                                           byte_context,
                                                                           0,
                                                                           ctraits,
                                                                           HighNibble{},
                                                                           local_coder,
                                                                           lbk,
                                                                           lit_high_priors);
               let byte_pull_status = drain_or_fill_static_buffer(LIT_CODER,
                                                                  local_coder,
                                                                  demuxer,
                                                                  muxer,
                                                                  output_bytes,
                                                                  output_offset,
                                                                  &mut Some(m8));
               low_buffer_warning = demuxer.data_ready(LIT_CODER as u8) < 16;
               h_nibble = cur_nibble;
               if let Some(prob) = cur_prob {
                   prob.blend(cur_nibble, lbk.literal_adaptation[0]);
               }
               if NibbleArrayType::FULLY_SAFE {
                   debug_assert!(match byte_pull_status {DivansResult::Success => true, _ => false,});
               } else {
                   match byte_pull_status {
                       DivansResult::Success => {},
                       need_something => {
                           retval = self.fallback_byte_encode(lc_target, cur_nibble, (start_byte_index + byte_offset) as u32 * 2 + 1, need_something);
                           break;
                       }
                   }
               }
           } else {
               h_nibble = *lc_target >> 4;
               low_buffer_warning = false;
               first = false;
           }
           let (l_nibble, l_prob) = self.code_nibble(byte_to_encode_val & 0xf,
                                                               byte_context,
                                                              h_nibble,
                                                              ctraits,
                                                              LowNibble{},
                                                              local_coder,
                                                              lbk,
                                                              lit_low_priors,
                                                              );
           let cur_byte = l_nibble | (h_nibble << 4);
           lbk.push_literal_byte(cur_byte);
           *lc_target = cur_byte;
           if let Some(prob) = l_prob {
               prob.blend(l_nibble, lbk.literal_adaptation[0]);
           }

           if NibbleArrayType::FULLY_SAFE && low_buffer_warning {
               let new_byte_index = start_byte_index + byte_offset + 1;
               if new_byte_index != last_llen as usize {
                  retval = DivansResult::NeedsMoreInput;
               }
               let new_state = self.state_literal_nibble_index((new_byte_index << 1) as u32,
                                                               demuxer.data_ready(LIT_CODER as u8));
               self.state = new_state;
               break;
            }
            let byte_pull_status = drain_or_fill_static_buffer(LIT_CODER,
                                                               local_coder,
                                                               demuxer,
                                                               muxer,
                                                               output_bytes,
                                                               output_offset,
                                                               &mut Some(m8));
            if NibbleArrayType::FULLY_SAFE {
                debug_assert!(match byte_pull_status {DivansResult::Success => true, _ => false,});
            } else {
                match byte_pull_status {
                  DivansResult::Success => {},
                  need_something => {
                      let new_state = self.state_literal_nibble_index(((start_byte_index + byte_offset) << 1) as u32 + 2,
                                                                       demuxer.data_ready(LIT_CODER as u8));
                      self.state = new_state;
                      if start_byte_index + byte_offset + 1 != last_llen as usize {
                          retval = need_something;
                      }
                      break;
                  }
               }
            }
        }
        self.lc.data = lc_data;
        retval
    }
    pub fn get_nibble_code_state<ISlice: SliceWrapper<u8>>(&self, index: u32, in_cmd: &LiteralCommand<ISlice>, bytes_rem:usize) -> LiteralSubstate {
        if in_cmd.prob.slice().is_empty() {
            self.state_literal_nibble_index(index, bytes_rem)
        } else {
            LiteralSubstate::LiteralNibbleIndexWithECDF(index)
        }
    }
    pub fn encode_or_decode<ISlice: SliceWrapper<u8>,
                            ArithmeticCoder:ArithmeticEncoderOrDecoder,
                            LinearInputBytes:StreamDemuxer<AllocU8>+Default,
                            LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                            Cdf16:CDF16,
                            Specialization:EncoderOrDecoderSpecialization,
                            AllocCDF16:Allocator<Cdf16>,
                            CTraits:CodecTraits,
                        >(&mut self,
                          superstate: &mut CrossCommandState<ArithmeticCoder,
                                                             Specialization,
                                                             LinearInputBytes,
                                                             LinearOutputBytes,
                                                             Cdf16,
                                                             AllocU8,
                                                             AllocCDF16>,
                          in_cmd: &LiteralCommand<ISlice>,
                          output_bytes:&mut [u8],
                          output_offset: &mut usize,
                          ctraits: &'static CTraits) -> DivansResult {
        let literal_len = in_cmd.data.slice().len() as u32;
        let serialized_large_literal_len  = literal_len.wrapping_sub(NUM_LITERAL_LENGTH_MNEMONIC + 1);
        let lllen: u8 = (core::mem::size_of_val(&serialized_large_literal_len) as u32 * 8 - serialized_large_literal_len.leading_zeros()) as u8;
        let _ltype = superstate.bk.get_literal_block_type();
        let (mut lit_coder, mut m8, mut lbk, mut lit_high_priors, mut lit_low_priors) = match superstate.thread_ctx {
            ThreadContext::Worker => (None, None, None, None, None),
            ThreadContext::MainThread(ref mut ctx) => (Some(&mut ctx.lit_coder),
                                                       Some(&mut ctx.m8),
                                                       Some(&mut ctx.lbk),
                                                       Some(&mut ctx.lit_high_priors),
                                                       Some(&mut ctx.lit_low_priors)),
        };
        
        loop {
            match m8 {
                Some(ref mut m) => {
                    match drain_or_fill_static_buffer(CMD_CODER,
                                                      &mut superstate.coder,
                                                      &mut superstate.demuxer, &mut superstate.muxer,
                                                      output_bytes, output_offset,
                                                      &mut Some(m.get_base_alloc())) {
                        DivansResult::Success => {},
                        needs_something => return needs_something,
                    }
                    match drain_or_fill_static_buffer(LIT_CODER,
                                                      *unwrap_ref!(lit_coder),
                                              &mut superstate.demuxer, &mut superstate.muxer,
                                                      output_bytes, output_offset,
                                                      &mut Some(m.get_base_alloc())) {
                        DivansResult::Success => {},
                        needs_something => return needs_something,
                    }
                },
                None => {
                    match drain_or_fill_static_buffer(CMD_CODER,
                                                      &mut superstate.coder,
                                                      &mut superstate.demuxer, &mut superstate.muxer,
                                                      output_bytes, output_offset,
                                                      &mut None) {
                        DivansResult::Success => {},
                        needs_something => return needs_something,
                    }
                    match drain_or_fill_static_buffer(LIT_CODER,
                                                      *unwrap_ref!(lit_coder),
                                              &mut superstate.demuxer, &mut superstate.muxer,
                                                      output_bytes, output_offset,
                                                      &mut None) {
                        DivansResult::Success => {},
                        needs_something => return needs_something,
                    }
                }
            }
            let billing = BillingDesignation::LiteralCommand(match self.state {
                LiteralSubstate::LiteralCountMantissaNibbles(_, _) => LiteralSubstate::LiteralCountMantissaNibbles(0, 0),
                LiteralSubstate::LiteralNibbleIndex(index) => LiteralSubstate::LiteralNibbleIndex(index % 2),
                LiteralSubstate::LiteralNibbleLowerHalf(index) => LiteralSubstate::LiteralNibbleIndex(index % 2),
                LiteralSubstate::LiteralNibbleIndexWithECDF(index) => LiteralSubstate::LiteralNibbleIndexWithECDF(index % 2),
                _ => self.state
            });
            match self.state {
                LiteralSubstate::Begin => {
                    self.state = LiteralSubstate::LiteralCountSmall(false);
                },
                LiteralSubstate::LiteralCountSmall(high_entropy_flag) => {
                    let index = 0;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut shortcut_nib = core::cmp::min(NUM_LITERAL_LENGTH_MNEMONIC, literal_len.wrapping_sub(1)) as u8;
                    if in_cmd.high_entropy && !high_entropy_flag {
                        shortcut_nib = NUM_LITERAL_LENGTH_MNEMONIC as u8 + 1;
                    }
                    let mut nibble_prob = superstate.bk.lit_len_priors.get(
                        LiteralCommandPriorType::CountSmall, (ctype, index));
                    superstate.coder.get_or_put_nibble(&mut shortcut_nib, nibble_prob, billing);
                    nibble_prob.blend(shortcut_nib, Speed::MED);// checked med

                    if shortcut_nib as u32 == NUM_LITERAL_LENGTH_MNEMONIC {
                        self.state = LiteralSubstate::LiteralCountFirst;
                    } else if shortcut_nib as u32 == 1 + NUM_LITERAL_LENGTH_MNEMONIC {
                        self.lc.high_entropy = true;
                        self.state = LiteralSubstate::LiteralCountSmall(true); // right now just 
                    } else {
                        let num_bytes = shortcut_nib as usize + 1;
                        superstate.bk.last_llen = num_bytes as u32;
                        //FIXME(threading): actually use the trait to get a new literal
                        self.lc.data = unwrap_ref!(m8).use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(num_bytes);
                        self.state = self.get_nibble_code_state(0, in_cmd,
                                                                superstate.demuxer.read_buffer()[LIT_CODER].bytes_avail());
                    }
                },
                LiteralSubstate::LiteralCountFirst => {
                    let mut beg_nib = core::cmp::min(15, lllen);
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.lit_len_priors.get(LiteralCommandPriorType::SizeBegNib, (ctype,));
                    superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    nibble_prob.blend(beg_nib, Speed::MUD);

                    if beg_nib == 15 {
                        self.state = LiteralSubstate::LiteralCountLengthGreater14Less25;
                    } else if beg_nib <= 1 {
                        self.lc.data = unwrap_ref!(m8).use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(
                            NUM_LITERAL_LENGTH_MNEMONIC as usize + 1 + beg_nib as usize);
                        self.state = self.get_nibble_code_state(0, in_cmd,
                                                                superstate.demuxer.read_buffer()[LIT_CODER].bytes_avail());
                    } else {
                        self.state = LiteralSubstate::LiteralCountMantissaNibbles(round_up_mod_4(beg_nib - 1),
                                                                                  1 << (beg_nib - 1));
                    }
                },
                LiteralSubstate::LiteralCountLengthGreater14Less25 => {
                    let mut last_nib = lllen.wrapping_sub(15);
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.lit_len_priors.get(LiteralCommandPriorType::SizeLastNib, (ctype,));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::MUD);

                    self.state = LiteralSubstate::LiteralCountMantissaNibbles(round_up_mod_4(last_nib + 14),
                                                                              1 << (last_nib + 14));
                },
                LiteralSubstate::LiteralCountMantissaNibbles(len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = (serialized_large_literal_len ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.lit_len_priors.get(LiteralCommandPriorType::SizeMantissaNib, (ctype,));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::MUD);
                    let next_decoded_so_far = decoded_so_far | (u32::from(last_nib) << next_len_remaining);

                    if next_len_remaining == 0 {
                        let num_bytes = next_decoded_so_far as usize + NUM_LITERAL_LENGTH_MNEMONIC as usize + 1;
                        superstate.bk.last_llen = num_bytes as u32;
                        //FIXME(threading): actually use the trait to alloc
                        self.lc.data = unwrap_ref!(m8).use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(
                            num_bytes);
                        self.state = self.get_nibble_code_state(0, in_cmd,
                                                                superstate.demuxer.read_buffer()[LIT_CODER].bytes_avail());
                    } else {
                        self.state  = LiteralSubstate::LiteralCountMantissaNibbles(next_len_remaining,
                                                                                   next_decoded_so_far);
                    }
                },
                LiteralSubstate::LiteralNibbleIndexWithECDF(nibble_index) => {
                    superstate.bk.last_llen = self.lc.data.slice().len() as u32;
                    let byte_index = (nibble_index as usize) >> 1;
                    let high_nibble = (nibble_index & 1) == 0;
                    let shift : u8 = if high_nibble { 4 } else { 0 };
                    let mut cur_nibble = (superstate.specialization.get_literal_byte(in_cmd, byte_index)
                                          >> shift) & 0xf;
                    assert!(in_cmd.prob.slice().is_empty() || (in_cmd.prob.slice().len() == 8 * in_cmd.data.slice().len()));
                    {
                        let prior_nibble;
                        {
                            prior_nibble = self.lc.data.slice()[byte_index];
                        }
                        cur_nibble = self.ecdf_write_nibble(nibble_index,
                                                            cur_nibble,
                                                            prior_nibble >> 4,
                                                            *unwrap_ref!(lit_coder),
                                                            Cdf16::default(),
                                                            in_cmd.prob.slice());
                                         
                        let cur_byte = &mut self.lc.data.slice_mut()[byte_index];
                        if shift ==0 {
                            *cur_byte |= cur_nibble << shift;
                        }else {
                            *cur_byte = cur_nibble << shift;
                        }
                        if !high_nibble {
                            unwrap_ref!(lbk).push_literal_byte(*cur_byte);
                        }
                    }
                    if nibble_index + 1 == (self.lc.data.slice().len() << 1) as u32 {
                        self.state = LiteralSubstate::FullyDecoded;
                        return DivansResult::Success;
                    } else {
                        self.state = LiteralSubstate::LiteralNibbleIndexWithECDF(nibble_index + 1);
                    }
                },
                LiteralSubstate::LiteralNibbleLowerHalf(nibble_index) => {
                    assert_eq!(nibble_index & 1, 1); // this is only for odd nibbles
                    let code_result = self.code_nibble_array(unwrap_ref!(m8).get_base_alloc(), output_bytes, output_offset,
                                                             in_cmd, nibble_index,
                                                             *unwrap_ref!(lit_coder), &mut superstate.demuxer, &mut superstate.muxer,
                                                             unwrap_ref!(lit_high_priors), unwrap_ref!(lit_low_priors),
                                                             unwrap_ref!(lbk),
                                                             &mut superstate.specialization, NibbleArraySecond{}, ctraits);
                    match code_result {
                        DivansResult::Success => {
                            self.state = LiteralSubstate::FullyDecoded;
                            return DivansResult::Success;
                        },
                        _ => return code_result,
                    }
                },
                LiteralSubstate::LiteralNibbleIndex(nibble_index) => {
                    let code_result = self.code_nibble_array(&mut unwrap_ref!(m8).get_base_alloc(), output_bytes, output_offset,
                                                             in_cmd, nibble_index,
                                                             *unwrap_ref!(lit_coder), &mut superstate.demuxer, &mut superstate.muxer,
                                                             unwrap_ref!(lit_high_priors), unwrap_ref!(lit_low_priors),
                                                             unwrap_ref!(lbk),
                                                             &mut superstate.specialization, NibbleArrayLowBuffer{}, ctraits);
                    match code_result {
                        DivansResult::Success => {
                            self.state = LiteralSubstate::FullyDecoded;
                            return DivansResult::Success;
                        },
                        _ => return code_result,
                    }
                },
                LiteralSubstate::SafeLiteralNibbleIndex(start_nibble_index) => {
                    match self.code_nibble_array(unwrap_ref!(m8).get_base_alloc(), output_bytes, output_offset,
                                                 in_cmd, start_nibble_index,
                                                 *unwrap_ref!(lit_coder), &mut superstate.demuxer, &mut superstate.muxer,
                                                 unwrap_ref!(lit_high_priors), unwrap_ref!(lit_low_priors),
                                                 unwrap_ref!(lbk),
                                                 &mut superstate.specialization, NibbleArraySafe{}, ctraits) {
                        DivansResult::NeedsMoreInput => {
                            continue;
                        }
                        DivansResult::Failure(m) => {
                            return DivansResult::Failure(m);
                        }
                        _ => {},
                    }
                    self.state = LiteralSubstate::FullyDecoded;
                    return DivansResult::Success;
                },
                LiteralSubstate::FullyDecoded => {
                    return DivansResult::Success;
                }
            }
        }
    }
    fn state_literal_nibble_index(&self, new_index: u32, bytes_rem: usize) -> LiteralSubstate {
        if bytes_rem >= 16 {
            LiteralSubstate::SafeLiteralNibbleIndex(new_index)
        }else {
            LiteralSubstate::LiteralNibbleIndex(new_index)
        }
    }
    #[cold]
    fn fallback_byte_encode(&mut self, lc_target: &mut u8, cur_nibble: u8, new_nibble_index: u32, res: DivansResult) -> DivansResult{
        *lc_target = cur_nibble << 4;
        self.state = LiteralSubstate::LiteralNibbleLowerHalf(new_nibble_index);
        res
    }
}
