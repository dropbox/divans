use core;
use brotli::BrotliResult;
use ::probability::{CDF2, CDF16, Speed, ExternalProbCDF16};
use ::constants;
use super::priors::LiteralNibblePriorType;
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    AllocatedMemoryPrefix,
    round_up_mod_4,
};
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    LiteralCommand,
    LiteralPredictionModeNibble,
    LITERAL_PREDICTION_MODE_SIGN,
    LITERAL_PREDICTION_MODE_UTF8,
    LITERAL_PREDICTION_MODE_MSB6,
    LITERAL_PREDICTION_MODE_LSB6,
};
use ::priors::PriorCollection;
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LiteralSubstate {
    Begin,
    LiteralCountSmall,
    LiteralCountFirst,
    LiteralCountLengthGreater14Less25,
    LiteralCountMantissaNibbles(u8, u32),
    LiteralNibbleIndex(u32),
    FullyDecoded,
}

pub struct LiteralState<AllocU8:Allocator<u8>> {
    pub lc:LiteralCommand<AllocatedMemoryPrefix<AllocU8>>,
    pub state: LiteralSubstate,
}

impl<AllocU8:Allocator<u8>,
                         > LiteralState<AllocU8> {
    pub fn encode_or_decode<ISlice: SliceWrapper<u8>,
                        ArithmeticCoder:ArithmeticEncoderOrDecoder,
                        Cdf16:CDF16,
                        Specialization:EncoderOrDecoderSpecialization,
                        AllocCDF2:Allocator<CDF2>,
                        AllocCDF16:Allocator<Cdf16>
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
                          output_offset: &mut usize) -> BrotliResult {
        let literal_len = in_cmd.data.slice().len() as u32;
        let serialized_large_literal_len  = literal_len.wrapping_sub(16);
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
                _ => self.state
            });
            match self.state {
                LiteralSubstate::Begin => {
                    self.state = LiteralSubstate::LiteralCountSmall;
                },
                LiteralSubstate::LiteralCountSmall => {
                    let index = 0;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut shortcut_nib = core::cmp::min(15, literal_len.wrapping_sub(1)) as u8;
                    let mut nibble_prob = superstate.bk.lit_priors.get(
                        LiteralNibblePriorType::CountSmall, (ctype, index));
                    superstate.coder.get_or_put_nibble(&mut shortcut_nib, nibble_prob, billing);
                    nibble_prob.blend(shortcut_nib, Speed::MED);// checked med

                    if shortcut_nib == 15 {
                        self.state = LiteralSubstate::LiteralCountFirst;
                    } else {
                        self.lc.data = AllocatedMemoryPrefix::<AllocU8>(superstate.m8.alloc_cell(shortcut_nib as usize + 1),
                                                                        shortcut_nib as usize + 1);
                        self.state = LiteralSubstate::LiteralNibbleIndex(0);
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
                        self.lc.data = AllocatedMemoryPrefix::<AllocU8>(superstate.m8.alloc_cell(16 + beg_nib as usize),
                                                                        16 + beg_nib as usize);
                        self.state = LiteralSubstate::LiteralNibbleIndex(0);
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
                        self.lc.data = AllocatedMemoryPrefix::<AllocU8>(superstate.m8.alloc_cell(next_decoded_so_far as usize + 16),
                                                                      next_decoded_so_far as usize+ 16);
                        self.state = LiteralSubstate::LiteralNibbleIndex(0);
                    } else {
                        self.state  = LiteralSubstate::LiteralCountMantissaNibbles(next_len_remaining,
                                                                                   next_decoded_so_far);
                    }
                },
                LiteralSubstate::LiteralNibbleIndex(nibble_index) => {
                    superstate.bk.last_llen = self.lc.data.slice().len() as u32;
                    let byte_index = (nibble_index as usize) >> 1;
                    let high_nibble = (nibble_index & 1) == 0;
                    let shift : u8 = if high_nibble { 4 } else { 0 };
                    let mut cur_nibble = (superstate.specialization.get_literal_byte(in_cmd, byte_index)
                                          >> shift) & 0xf;
	            let stride = core::cmp::max(1, superstate.bk.stride);
                    let base_shift = 0x40 - stride * 8;
                    let k0 = ((superstate.bk.last_8_literals >> (base_shift+4)) & 0xf) as usize;
                    let k1 = ((superstate.bk.last_8_literals >> base_shift) & 0xf) as usize;
                    assert!(in_cmd.prob.slice().is_empty() || (in_cmd.prob.slice().len() == 8 * in_cmd.data.slice().len()));
                    {
                        let cur_byte = &mut self.lc.data.slice_mut()[byte_index];
                        let selected_context:usize;
                        let actual_context: usize;
                        {
                            let nibble_index_truncated = core::cmp::min(nibble_index as usize, 0);
                            let prev_byte = ((superstate.bk.last_8_literals >> 0x38) & 0xff) as u8;
                            let prev_prev_byte = ((superstate.bk.last_8_literals >> 0x30) & 0xff) as u8;
                            let utf_context = constants::UTF8_CONTEXT_LOOKUP[prev_byte as usize]
                                | constants::UTF8_CONTEXT_LOOKUP[prev_prev_byte as usize + 256];
                            let sign_context =
                                (constants::SIGNED_3_BIT_CONTEXT_LOOKUP[prev_byte as usize] << 3) |
                                constants::SIGNED_3_BIT_CONTEXT_LOOKUP[prev_prev_byte as usize];
                            let msb_context = prev_byte >> 2;
                            let lsb_context = prev_byte & 0x3f;
                            selected_context = match superstate.bk.literal_prediction_mode {
                                LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_SIGN) => sign_context,
                                LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_UTF8) => utf_context,
                                LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_MSB6) => msb_context,
                                LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_LSB6) => lsb_context,
                                _ => panic!("Internal Error: parsed nibble prediction mode has more than 2 bits"),
                            } as usize;
                            let cmap_index = selected_context as usize + 64 * superstate.bk.get_literal_block_type() as usize;
                            actual_context = if superstate.bk.materialized_prediction_mode() {
                                superstate.bk.literal_context_map.slice()[cmap_index as usize] as usize
                            } else {
                                selected_context
                            };
                            //if shift != 0 {
                            //println_stderr!("___{}{}{}",
                            //                prev_prev_byte as u8 as char,
                            //                prev_byte as u8 as char,
                            //                superstate.specialization.get_literal_byte(in_cmd, byte_index) as char);
                            //                }
                            let materialized_prediction_mode = superstate.bk.materialized_prediction_mode();
                            let mut nibble_prob = if high_nibble {
                                superstate.bk.lit_priors.get(LiteralNibblePriorType::FirstNibble,
                                                             (superstate.bk.stride as usize,
                                                              actual_context,
                                                              k0 * 16 + k1,
                                                              nibble_index_truncated))
                            } else {
                                let cur_byte_prior = (*cur_byte >> 4) as usize;
                                superstate.bk.lit_priors.get(LiteralNibblePriorType::SecondNibble,
                                                             (superstate.bk.stride as usize,
                                                              cur_byte_prior,
                                                              k0 * 16 + k1,
                                                              nibble_index_truncated))
                            };
                            let mut cm_prob = if high_nibble {
                                superstate.bk.lit_cm_priors.get(LiteralNibblePriorType::FirstNibble,
                                                             (0,
                                                              actual_context,
                                                              0,
                                                              nibble_index_truncated))
                            } else {
                                let cur_byte_prior = (*cur_byte >> 4) as usize;
                                superstate.bk.lit_cm_priors.get(LiteralNibblePriorType::SecondNibble,
                                                             (0,
                                                              actual_context,
                                                              cur_byte_prior,
                                                              nibble_index_truncated))
                            };
                            let prob = if materialized_prediction_mode {
                                if superstate.bk.combine_literal_predictions {
                                    cm_prob.average(nibble_prob, superstate.bk.model_weights[high_nibble as usize].norm_weight() as u16 as i32)
                                } else {
                                    *cm_prob
                                }
                            } else {
                                *nibble_prob
                            };
                            let mut ecdf = ExternalProbCDF16::default();
                            let shift_offset = if shift != 0 { 0usize } else { 4usize };
                            let en = byte_index*8 + shift_offset + 4;
                            let weighted_prob_range = if en <= in_cmd.prob.slice().len() {
                                let st = en - 4;
                                let probs = [in_cmd.prob.slice()[st], in_cmd.prob.slice()[st + 1],
                                             in_cmd.prob.slice()[st + 2], in_cmd.prob.slice()[st + 3]];
                                ecdf.init(cur_nibble, &probs, nibble_prob);
                                superstate.coder.get_or_put_nibble(&mut cur_nibble, &ecdf, billing)
                            } else {
                                superstate.coder.get_or_put_nibble(&mut cur_nibble, &prob, billing)
                            };
                            if materialized_prediction_mode && superstate.bk.model_weights[high_nibble as usize].should_mix() {
                                let model_probs = [
                                    cm_prob.sym_to_start_and_freq(cur_nibble).range.freq,
                                    nibble_prob.sym_to_start_and_freq(cur_nibble).range.freq,
                                ];
                                superstate.bk.model_weights[high_nibble as usize].update(model_probs, weighted_prob_range.freq);
                            }
                            nibble_prob.blend(cur_nibble, superstate.bk.literal_adaptation.clone());
                            cm_prob.blend(cur_nibble, Speed::GLACIAL);
                        }
                        *cur_byte |= cur_nibble << shift;
                        if !high_nibble {
                            superstate.bk.push_literal_byte(*cur_byte);
                            //println_stderr!("L {:},{:} = {:} for {:02x}",
                            //                selected_context,
                            //                superstate.bk.get_literal_block_type(),
                            //                actual_context,
                            //                *cur_byte);
                        }
                    }

                    /*
                        println_stderr!("{}{}",
                                        //((_k7<<4)|_k8) as u8 as char,
                                        //((_k5<<4)|_k6) as u8 as char,
                                        //((_k3<<4)|_k4) as u8 as char,
                                        ((k0<<4)|k1) as u8 as char,
                                        self.lc.data.slice_mut()[byte_index] as char);
                    */

                    if nibble_index + 1 == (self.lc.data.slice().len() << 1) as u32 {
                        self.state = LiteralSubstate::FullyDecoded;
                        return BrotliResult::ResultSuccess;
                    } else {
                        self.state = LiteralSubstate::LiteralNibbleIndex(nibble_index + 1);
                    }
                },
                LiteralSubstate::FullyDecoded => {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
}

