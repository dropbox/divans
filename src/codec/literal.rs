use core;
use brotli::BrotliResult;
use ::probability::{CDF2, CDF16, Speed, ExternalProbCDF16};
use super::priors::LiteralNibblePriorType;
use ::slice_util::AllocatedMemoryPrefix;
use ::alloc_util::UninitializedOnAlloc;
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    ByteContext,
    round_up_mod_4,
    CrossCommandBookKeeping,
};
use super::specializations::CodecTraits;
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    LiteralCommand,
};
use ::priors::PriorCollection;
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LiteralSubstate {
    Begin,
    LiteralCountSmall(bool),
    LiteralCountFirst,
    LiteralCountLengthGreater14Less25,
    LiteralCountMantissaNibbles(u8, u32),
    LiteralNibbleIndex(u32),
    LiteralNibbleLowerHalf(u32),
    LiteralNibbleIndexWithECDF(u32),
    FullyDecoded,
}

const NUM_LITERAL_LENGTH_MNEMONIC: u32 = 14;
pub struct LiteralState<AllocU8:Allocator<u8>> {
    pub lc:LiteralCommand<AllocatedMemoryPrefix<u8, AllocU8>>,
    pub state: LiteralSubstate,
}

#[inline(always)]
pub fn get_prev_word_context<Cdf16:CDF16,
                             AllocU8:Allocator<u8>,
                             AllocCDF2:Allocator<CDF2>,
                             AllocCDF16:Allocator<Cdf16>,
                             CTraits:CodecTraits>(bk: &CrossCommandBookKeeping<Cdf16,
                                                                              AllocU8,
                                                                              AllocCDF2,
                                                                               AllocCDF16>,
                                                  _ctraits: &'static CTraits) -> ByteContext {
    let local_stride = if CTraits::HAVE_STRIDE { bk.stride } else {1};
    let base_shift = 0x40 - local_stride * 8;
    let stride_byte = ((bk.last_8_literals >> base_shift) & 0xff) as u8;
    let prev_byte = ((bk.last_8_literals >> 0x38) & 0xff) as u8;
    let prev_prev_byte = ((bk.last_8_literals >> 0x30) & 0xff) as u8;
    let selected_context = bk.literal_lut0[prev_byte as usize] | bk.literal_lut1[prev_prev_byte as usize];
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
    debug_assert_eq!(bk.materialized_prediction_mode(), CTraits::MATERIALIZED_PREDICTION_MODE);
    let actual_context = if CTraits::MATERIALIZED_PREDICTION_MODE {
        let cmap_index = selected_context as usize + ((bk.get_literal_block_type() as usize) << 6);
        bk.literal_context_map.slice()[cmap_index as usize]
    } else {
        selected_context
    };
    ByteContext{actual_context:actual_context, stride_byte: stride_byte}
}


impl<AllocU8:Allocator<u8>,
                         > LiteralState<AllocU8> {
    pub fn ecdf_write_nibble<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                        Cdf16:CDF16,
                        Specialization:EncoderOrDecoderSpecialization,
                        AllocCDF2:Allocator<CDF2>,
                        AllocCDF16:Allocator<Cdf16>
                       >(&mut self,
                         nibble_index: u32,
                         mut cur_nibble: u8,
                         _cur_byte_prior: u8,
                          superstate: &mut CrossCommandState<ArithmeticCoder,
                                                             Specialization,
                                                             Cdf16,
                                                             AllocU8,
                                                             AllocCDF2,
                                                             AllocCDF16>,
                         in_cmd_prob_slice: &[u8]) -> u8 {
        let high_nibble = (nibble_index & 1) == 0;
        let mut ecdf = ExternalProbCDF16::default();
        let shift_offset = if high_nibble { 4usize } else { 0usize };
        let byte_index = (nibble_index as usize) >> 1;
        let en = byte_index*8 + shift_offset + 4;
        if en <= in_cmd_prob_slice.len() {
            let nibble_prob = Cdf16::default();
            let st = en - 4;
            let probs = [in_cmd_prob_slice[st], in_cmd_prob_slice[st + 1],
                             in_cmd_prob_slice[st + 2], in_cmd_prob_slice[st + 3]];
            ecdf.init(cur_nibble, &probs, &nibble_prob);
            superstate.coder.get_or_put_nibble(&mut cur_nibble, &ecdf, BillingDesignation::LiteralCommand(LiteralSubstate::LiteralNibbleIndex(nibble_index & 1)));
        } else {
            superstate.coder.get_or_put_nibble(&mut cur_nibble, &ecdf, BillingDesignation::LiteralCommand(LiteralSubstate::LiteralNibbleIndex(nibble_index & 1)));
        }
        cur_nibble
    }
    #[inline(always)]
    pub fn code_nibble<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                       Cdf16:CDF16,
                       Specialization:EncoderOrDecoderSpecialization,
                       AllocCDF2:Allocator<CDF2>,
                       AllocCDF16:Allocator<Cdf16>,
                       CTraits:CodecTraits,
                       >(&mut self,
                         high_nibble: bool,
                         mut cur_nibble: u8,
                         byte_context: ByteContext,
                         cur_byte_prior: u8,
                         _high_entropy: bool,
                         _ctraits: &'static CTraits,
                         superstate: &mut CrossCommandState<ArithmeticCoder,
                                                            Specialization,
                                                            Cdf16,
                                                            AllocU8,
                                                            AllocCDF2,
                                                            AllocCDF16>) -> u8 {
        debug_assert_eq!(CTraits::MATERIALIZED_PREDICTION_MODE, superstate.bk.materialized_prediction_mode());
        let stride_xor = if CTraits::HAVE_STRIDE {(superstate.bk.stride as usize - 1) << 4} else {0};
        let mut mixing_mask_index = byte_context.actual_context as usize;
        if !high_nibble {
            mixing_mask_index += 256 * (1 + (cur_byte_prior as usize & 0xf));
        }
        let mm_opts = (superstate.bk.mixing_mask[(mixing_mask_index >> 5)] >> ((mixing_mask_index & 31) * 2)) & 3;
        let is_mm = (mm_opts & 1) as usize; 
        let mm = -(is_mm as isize) as usize;
        let nibble_prob = if high_nibble {
            superstate.bk.lit_priors.get(LiteralNibblePriorType::FirstNibble,
                                         (byte_context.stride_byte as usize & mm,
                                          byte_context.actual_context as usize ^ stride_xor,
                                          0,
                                          ))
        } else {
            superstate.bk.lit_priors.get(LiteralNibblePriorType::SecondNibble,
                                         ((mm & byte_context.stride_byte as usize) | (!mm & byte_context.actual_context as usize),
                                          cur_byte_prior as usize,
                                          mm & 1,
                                          ))
        };
        
        let cm_prob = if high_nibble {
            superstate.bk.lit_cm_priors.get(LiteralNibblePriorType::FirstNibble,
                                            (0,//(byte_context.selected_context as i8 & -(superstate.bk.prior_depth as i8)) as usize,
                                             byte_context.actual_context as usize,))
        } else {
            superstate.bk.lit_cm_priors.get(LiteralNibblePriorType::SecondNibble,
                                            (0,//(byte_context.selected_context as i8 & -(superstate.bk.prior_depth as i8)) as usize,
                                             cur_byte_prior as usize,
                                             byte_context.actual_context as usize))
        };
        let prob = if CTraits::MATERIALIZED_PREDICTION_MODE && CTraits::COMBINE_LITERAL_PREDICTIONS && !superstate.bk.model_weights[high_nibble as usize].should_mix() {
            *nibble_prob
        } else if CTraits::MATERIALIZED_PREDICTION_MODE {
            debug_assert_eq!(CTraits::COMBINE_LITERAL_PREDICTIONS, superstate.bk.combine_literal_predictions);
            if CTraits::COMBINE_LITERAL_PREDICTIONS {
                //debug_assert_eq!(superstate.bk.model_weights[high_nibble as usize].should_mix(),
                //                 CTraits::SHOULD_MIX);
                cm_prob.average(nibble_prob, superstate.bk.model_weights[high_nibble as usize].norm_weight() as u16 as i32)
            } else {
                *cm_prob
            }
        } else {
            *nibble_prob
        };
        let weighted_prob_range = superstate.coder.get_or_put_nibble(&mut cur_nibble,
                                                                     &prob,
                                                                     BillingDesignation::LiteralCommand(LiteralSubstate::LiteralNibbleIndex(!high_nibble as u32)));

        if CTraits::MATERIALIZED_PREDICTION_MODE && CTraits::COMBINE_LITERAL_PREDICTIONS && superstate.bk.model_weights[high_nibble as usize].should_mix() {
            let model_probs = [
                cm_prob.sym_to_start_and_freq(cur_nibble).range.freq,
                nibble_prob.sym_to_start_and_freq(cur_nibble).range.freq,
            ];
            superstate.bk.model_weights[high_nibble as usize].update(model_probs, weighted_prob_range.freq);
        }
        if CTraits::COMBINE_LITERAL_PREDICTIONS || !CTraits::MATERIALIZED_PREDICTION_MODE {
            nibble_prob.blend(cur_nibble, superstate.bk.literal_adaptation[(((!is_mm)&1) << 1) | high_nibble as usize].clone());
        }
        if CTraits::MATERIALIZED_PREDICTION_MODE {
            cm_prob.blend(cur_nibble, superstate.bk.literal_adaptation[2 | high_nibble as usize].clone());
        }
        cur_nibble
    }
    pub fn get_nibble_code_state<ISlice: SliceWrapper<u8>>(&self, index: u32, in_cmd: &LiteralCommand<ISlice>) -> LiteralSubstate {
        if in_cmd.prob.slice().is_empty() {
            LiteralSubstate::LiteralNibbleIndex(index)
        } else {
            LiteralSubstate::LiteralNibbleIndexWithECDF(index)
        }
    }
    pub fn encode_or_decode<ISlice: SliceWrapper<u8>,
                            ArithmeticCoder:ArithmeticEncoderOrDecoder,
                            Cdf16:CDF16,
                            Specialization:EncoderOrDecoderSpecialization,
                            AllocCDF2:Allocator<CDF2>,
                            AllocCDF16:Allocator<Cdf16>,
                            CTraits:CodecTraits,
                        >(&mut self,
                          superstate: &mut CrossCommandState<ArithmeticCoder,
                                                             Specialization,
                                                             Cdf16,
                                                             AllocU8,
                                                             AllocCDF2,
                                                             AllocCDF16>,
                          in_cmd: &LiteralCommand<ISlice>,
                          input_bytes:&[u8],
                          input_offset: &mut usize,
                          output_bytes:&mut [u8],
                          output_offset: &mut usize,
                          ctraits: &'static CTraits) -> BrotliResult {
        let literal_len = in_cmd.data.slice().len() as u32;
        let serialized_large_literal_len  = literal_len.wrapping_sub(NUM_LITERAL_LENGTH_MNEMONIC + 1);
        let lllen: u8 = (core::mem::size_of_val(&serialized_large_literal_len) as u32 * 8 - serialized_large_literal_len.leading_zeros()) as u8;
        let _ltype = superstate.bk.get_literal_block_type();
        loop {
            match superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset) {
                BrotliResult::ResultSuccess => {},
                need_something => return need_something,
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
                    let mut nibble_prob = superstate.bk.lit_priors.get(
                        LiteralNibblePriorType::CountSmall, (ctype, index));
                    superstate.coder.get_or_put_nibble(&mut shortcut_nib, nibble_prob, billing);
                    nibble_prob.blend(shortcut_nib, Speed::MED);// checked med

                    if shortcut_nib as u32 == NUM_LITERAL_LENGTH_MNEMONIC {
                        self.state = LiteralSubstate::LiteralCountFirst;
                    } else if shortcut_nib as u32 == 1 + NUM_LITERAL_LENGTH_MNEMONIC {
                        self.lc.high_entropy = true;
                        self.state = LiteralSubstate::LiteralCountSmall(true); // right now just 
                    } else {
                        self.lc.data = superstate.m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(shortcut_nib as usize + 1);
                        self.state = self.get_nibble_code_state(0, in_cmd);
                    }
                },
                LiteralSubstate::LiteralCountFirst => {
                    let mut beg_nib = core::cmp::min(15, lllen);
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.lit_priors.get(LiteralNibblePriorType::SizeBegNib, (ctype,));
                    superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    nibble_prob.blend(beg_nib, Speed::MUD);

                    if beg_nib == 15 {
                        self.state = LiteralSubstate::LiteralCountLengthGreater14Less25;
                    } else if beg_nib <= 1 {
                        self.lc.data = superstate.m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(
                            NUM_LITERAL_LENGTH_MNEMONIC as usize + 1 + beg_nib as usize);
                        self.state = self.get_nibble_code_state(0, in_cmd);
                    } else {
                        self.state = LiteralSubstate::LiteralCountMantissaNibbles(round_up_mod_4(beg_nib - 1),
                                                                                  1 << (beg_nib - 1));
                    }
                },
                LiteralSubstate::LiteralCountLengthGreater14Less25 => {
                    let mut last_nib = lllen.wrapping_sub(15);
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.lit_priors.get(LiteralNibblePriorType::SizeLastNib, (ctype,));
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
                    let mut nibble_prob = superstate.bk.lit_priors.get(LiteralNibblePriorType::SizeMantissaNib, (ctype,));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::MUD);
                    let next_decoded_so_far = decoded_so_far | (u32::from(last_nib) << next_len_remaining);

                    if next_len_remaining == 0 {
                        self.lc.data = superstate.m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(
                            next_decoded_so_far as usize + NUM_LITERAL_LENGTH_MNEMONIC as usize + 1);
                        self.state = self.get_nibble_code_state(0, in_cmd);
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
                                                                superstate,
                                                                in_cmd.prob.slice());
                                         
                        let cur_byte = &mut self.lc.data.slice_mut()[byte_index];
                        if shift ==0 {
                            *cur_byte |= cur_nibble << shift;
                        }else {
                            *cur_byte = cur_nibble << shift;
                        }
                        if !high_nibble {
                            superstate.bk.push_literal_byte(*cur_byte);
                        }
                    }
                    if nibble_index + 1 == (self.lc.data.slice().len() << 1) as u32 {
                        self.state = LiteralSubstate::FullyDecoded;
                        return BrotliResult::ResultSuccess;
                    } else {
                        self.state = LiteralSubstate::LiteralNibbleIndexWithECDF(nibble_index + 1);
                    }
                },
                LiteralSubstate::LiteralNibbleLowerHalf(nibble_index) => {
                    assert_eq!(nibble_index & 1, 1); // this is only for odd nibbles
                    let high_entropy = self.lc.high_entropy;
                    let byte_index = (nibble_index as usize) >> 1;
                    let mut byte_to_encode_val = superstate.specialization.get_literal_byte(in_cmd, byte_index);
                    let byte_context = get_prev_word_context(&superstate.bk,
                                                             ctraits);
                    {
                        let prior_nibble = self.lc.data.slice()[byte_index];
                        let cur_nibble = self.code_nibble(false,
                                                          byte_to_encode_val & 0xf,
                                                          byte_context,
                                                          prior_nibble >> 4,
                                                          high_entropy,
                                                          ctraits,
                                                          superstate,
                                                          );
                        let cur_byte = &mut self.lc.data.slice_mut()[byte_index];
                        *cur_byte = cur_nibble | *cur_byte;
                        superstate.bk.push_literal_byte(*cur_byte);
                    }
                    if byte_index + 1 == self.lc.data.slice().len() {
                        self.state = LiteralSubstate::FullyDecoded;
                        return BrotliResult::ResultSuccess;
                    } else {
                        self.state = LiteralSubstate::LiteralNibbleIndex(nibble_index + 1);
                    }
                },
                LiteralSubstate::LiteralNibbleIndex(nibble_index) => {
                    superstate.bk.last_llen = self.lc.data.slice().len() as u32;
                    let high_entropy = self.lc.high_entropy;
                    let byte_index = (nibble_index as usize) >> 1;
                    let mut byte_to_encode_val = superstate.specialization.get_literal_byte(in_cmd, byte_index);
                    let byte_context = get_prev_word_context(&superstate.bk, ctraits);
                    let cur_nibble = self.code_nibble(true,
                                                      byte_to_encode_val >> 4,
                                                      byte_context,
                                                      0,
                                                      high_entropy,
                                                      ctraits,
                                                      superstate,
                                                      );
                    match superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset) {
                        BrotliResult::ResultSuccess => {},
                        need_something => {
                            return self.fallback_byte_encode(cur_nibble, nibble_index, need_something);
                        }
                    }
                    let cur_byte = self.code_nibble(false,
                                                    byte_to_encode_val & 0xf,
                                                    byte_context,
                                                    cur_nibble,
                                                    high_entropy,
                                                    ctraits,
                                                    superstate,
                                                    ) | (cur_nibble << 4);
                    self.lc.data.slice_mut()[byte_index] = cur_byte;
                    superstate.bk.push_literal_byte(cur_byte);
                    if byte_index + 1 == self.lc.data.slice().len() {
                        self.state = LiteralSubstate::FullyDecoded;
                        return BrotliResult::ResultSuccess;
                    } else {
                        self.state = LiteralSubstate::LiteralNibbleIndex(nibble_index + 2);
                    }
                },
                LiteralSubstate::FullyDecoded => {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
    #[cold]
    fn fallback_byte_encode(&mut self, cur_nibble: u8, nibble_index: u32, res: BrotliResult) -> BrotliResult{
        self.lc.data.slice_mut()[(nibble_index >> 1) as usize] = cur_nibble << 4;
        self.state = LiteralSubstate::LiteralNibbleLowerHalf(nibble_index + 1);
        res
    }
}
