use core;
use alloc::Allocator;
use interface::{DivansResult, ErrMsg, StreamMuxer, StreamDemuxer};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    round_up_mod_4,
    CMD_CODER,
};
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    CopyCommand,
};
use ::priors::PriorCollection;
use ::probability::{Speed, CDF16, CDF2};
use super::priors::CopyCommandNibblePriorType;
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CopySubstate {
    Begin,
    CountSmall,
    CountLengthFirst,
    CountLengthGreater18Less25, // length not between 0 and 14 inclusive... second nibble results in 15-24
    CountMantissaNibbles(u8, u8, u32), //nibble count, intermediate result
    CountDecoded,
    DistanceLengthMnemonic, // references a recent distance cached value
    DistanceLengthFirst,
    DistanceLengthGreater15Less25, // length not between 1 and 15, inclusive.. second nibble results in 15-24
    DistanceMantissaNibbles(u8, u8, u32), // nibble count (up to 6), intermediate result
    FullyDecoded,
}
pub struct CopyState {
   pub cc:CopyCommand,
   pub state: CopySubstate,
}



impl CopyState {
    pub fn begin() -> Self {
        CopyState{
            cc: CopyCommand {
                distance:0,
                num_bytes:0,
            },
            state:CopySubstate::Begin,
        }
    }
    #[cfg_attr(not(feature="no-inline"), inline(always))]
    pub fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                            Specialization:EncoderOrDecoderSpecialization,
                            LinearInputBytes:StreamDemuxer<AllocU8>+Default,
                            LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                             
                        Cdf16:CDF16,
                        AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<CDF2>,
                        AllocCDF16:Allocator<Cdf16>>(&mut self,
                                                     superstate: &mut CrossCommandState<ArithmeticCoder,
                                                                                        Specialization,
                                                                                        LinearInputBytes,
                                                                                        LinearOutputBytes,
                                                                                        Cdf16,
                                                                                        AllocU8,
                                                                                        AllocCDF2,
                                                                                        AllocCDF16>,
                                                     in_cmd: &CopyCommand,
                                                     output_bytes:&mut [u8],
                                                     output_offset: &mut usize) -> DivansResult {
        let dlen: u8 = (core::mem::size_of_val(&in_cmd.distance) as u32 * 8 - in_cmd.distance.leading_zeros()) as u8;
        let clen: u8 = (core::mem::size_of_val(&in_cmd.num_bytes) as u32 * 8 - in_cmd.num_bytes.leading_zeros()) as u8;
        if dlen ==0 {
            return DivansResult::Failure(ErrMsg::Distance0NotAllowed); // not allowed to copy from 0 distance
        }
        loop {
            match superstate.drain_or_fill_internal_buffer(CMD_CODER, output_bytes, output_offset) {
                DivansResult::Success => {},
                need_something => return need_something,
            }
            let billing = BillingDesignation::CopyCommand(match self.state {
                CopySubstate::CountMantissaNibbles(_, _, _) => CopySubstate::CountMantissaNibbles(0, 0, 0),
                CopySubstate::DistanceMantissaNibbles(_, _, _) => CopySubstate::DistanceMantissaNibbles(0, 0, 0),
                _ => self.state
            });
            match self.state {
                CopySubstate::Begin => {
                    self.state = CopySubstate::CountSmall;
                },
                CopySubstate::CountSmall => {
                    let index = ((superstate.bk.last_4_states >> 4) & 3) as usize + 4 * core::cmp::min(superstate.bk.last_llen - 1, 3) as usize;
                    let mut shortcut_nib = core::cmp::min(15, in_cmd.num_bytes) as u8;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountSmall, (ctype, index));
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut shortcut_nib, nibble_prob, billing);
                    nibble_prob.blend(shortcut_nib, Speed::MUD);

                    if shortcut_nib == 15 {
                        self.state = CopySubstate::CountLengthFirst;
                    } else {
                        self.cc.num_bytes = u32::from(shortcut_nib);
                        superstate.bk.last_clen = (core::mem::size_of_val(&self.cc.num_bytes) as u32 * 8
                                                   - (self.cc.num_bytes).leading_zeros()) as u8;
                        self.state = CopySubstate::CountDecoded;
                    }
                },
                CopySubstate::CountLengthFirst => {
                    // at this point, num_bytes is at least 15, so clen is at least 4.
                    let mut beg_nib = core::cmp::min(15, clen.wrapping_sub(4));
                    let index = 0;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountBegNib, (ctype, index));
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    nibble_prob.blend(beg_nib, Speed::FAST);

                    if beg_nib == 15 {
                        self.state = CopySubstate::CountLengthGreater18Less25;
                    } else {
                        superstate.bk.last_clen = beg_nib + 4;
                        self.state = CopySubstate::CountMantissaNibbles(0, round_up_mod_4(beg_nib + 4 - 1), 1 << (beg_nib + 4 - 1));
                    }
                },
                CopySubstate::CountLengthGreater18Less25 => {
                    let mut last_nib = clen.wrapping_sub(19);
                    let index = 0;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountLastNib, (ctype, index));
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::FAST);
                    superstate.bk.last_clen = last_nib + 19;
                    self.state = CopySubstate::CountMantissaNibbles(0, round_up_mod_4(last_nib + 18), 1 << (last_nib + 18));
                },
                CopySubstate::CountMantissaNibbles(len_decoded, len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = (in_cmd.num_bytes ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    let index = if len_decoded == 0 { ((superstate.bk.last_clen % 4) + 1) as usize } else { 0usize };
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountMantissaNib, (ctype, index));
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    let next_decoded_so_far = decoded_so_far | (u32::from(last_nib) << next_len_remaining);
                    nibble_prob.blend(last_nib, Speed::SLOW);

                    if next_len_remaining == 0 {
                        self.cc.num_bytes = next_decoded_so_far;
                        self.state = CopySubstate::CountDecoded;
                    } else {
                        self.state  = CopySubstate::CountMantissaNibbles(
                            len_decoded + 4,
                            next_len_remaining,
                            next_decoded_so_far);
                    }
                },
                CopySubstate::CountDecoded => {
                    self.state = CopySubstate::DistanceLengthMnemonic;
                },
                CopySubstate::DistanceLengthMnemonic => {
                    let mut beg_nib = if Specialization::IS_DECODING_FILE {
                        15 // we can't search for mnemonic in empty in_cmd (not yet decoded)
                    } else {
                        superstate.bk.distance_mnemonic_code(in_cmd.distance)
                    };
                    //let index = 0;
                    let actual_prior = superstate.bk.get_distance_prior(self.cc.num_bytes);
                    {
                        let mut nibble_prob = superstate.bk.copy_priors.get(
                            CopyCommandNibblePriorType::DistanceMnemonic, (actual_prior as usize, ((superstate.bk.last_llen < 8) as usize)));
                        superstate.coder[CMD_CODER].get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                        nibble_prob.blend(beg_nib, Speed::SLOW);
                    }
                    //println_stderr!("D {},{} => {} as {}", dtype, distance_map_index, actual_prior, beg_nib);
                    if beg_nib == 15 {
                        self.state = CopySubstate::DistanceLengthFirst;
                    } else {
                        let (dist, ok) = superstate.bk.get_distance_from_mnemonic_code(beg_nib);
                        self.cc.distance = dist;
                        superstate.bk.last_dlen = (core::mem::size_of_val(&self.cc.distance) as u32 * 8
                                                   - self.cc.distance.leading_zeros()) as u8;
                        if !ok {
                            return DivansResult::Failure(ErrMsg::CopyDistanceMnemonicCodeBad(dist as u8, (dist >> 8) as u8));
                        }
                        self.state = CopySubstate::FullyDecoded;
                    }
                },
                CopySubstate::DistanceLengthFirst => {
                    let mut beg_nib = core::cmp::min(15, dlen - 1);
                    let index = (core::mem::size_of_val(&self.cc.num_bytes) as u32 * 8 - self.cc.num_bytes.leading_zeros()) as usize >> 2;
                    let actual_prior = superstate.bk.get_distance_prior(self.cc.num_bytes);
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::DistanceBegNib, (actual_prior as usize, index));
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    nibble_prob.blend(beg_nib, Speed::SLOW);
                    if beg_nib == 15 {
                        self.state = CopySubstate::DistanceLengthGreater15Less25;
                    } else {
                        superstate.bk.last_dlen = beg_nib + 1;
                        if beg_nib == 0 {
                            self.cc.distance = 1;
                            self.state = CopySubstate::FullyDecoded;
                        } else {
                            self.state = CopySubstate::DistanceMantissaNibbles(0, round_up_mod_4(beg_nib), 1 << beg_nib);
                        }
                    }
                },
                CopySubstate::DistanceLengthGreater15Less25 => {
                    let mut last_nib = dlen.wrapping_sub(16);
                    let index = 0;
                    let actual_prior = superstate.bk.get_distance_prior(self.cc.num_bytes);
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::DistanceLastNib, (actual_prior, index));
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::ROCKET);
                    superstate.bk.last_dlen = (last_nib + 15) + 1;
                    self.state = CopySubstate::DistanceMantissaNibbles(0, round_up_mod_4(last_nib + 15), 1 << (last_nib + 15));
                },
                CopySubstate::DistanceMantissaNibbles(mut len_decoded, start_len_remaining, mut decoded_so_far) => {
                    for next_len_remaining_sr2 in (0..((start_len_remaining as usize + 3) >> 2)).rev() {
                        let next_len_remaining = (next_len_remaining_sr2 as u8) << 2;
                        let actual_prior = superstate.bk.get_distance_prior(self.cc.num_bytes);
                        let last_nib_as_u32 = (in_cmd.distance ^ decoded_so_far) >> next_len_remaining;
                        let mut last_nib = last_nib_as_u32 as u8;
                        let index = if len_decoded == 0 { ((superstate.bk.last_dlen & 3) + 1) as usize } else { 0usize };
                        let four_if_0_or_1_64_if_2_3_or_4 = 0x4 << ((index & 6) << ((index & 2)>>1));
                        let next_decoded_so_far;
                        {
                            let mut nibble_prob = superstate.bk.copy_priors.get(
                                CopyCommandNibblePriorType::DistanceMantissaNib, (actual_prior, index));
                            superstate.coder[CMD_CODER].get_or_put_nibble(&mut last_nib, nibble_prob, BillingDesignation::CopyCommand(
                                CopySubstate::DistanceMantissaNibbles(0, 0, 0)));
                            next_decoded_so_far = decoded_so_far | (u32::from(last_nib) << next_len_remaining);
                            nibble_prob.blend(last_nib, Speed::new(four_if_0_or_1_64_if_2_3_or_4, 0x4000));
                        }
                        match superstate.drain_or_fill_internal_buffer(CMD_CODER, output_bytes, output_offset) {
                            DivansResult::Success => {},
                            need_something => {
                                if next_len_remaining == 0 {
                                    self.cc.distance = next_decoded_so_far;
                                    self.state = CopySubstate::FullyDecoded;
                                } else {
                                    self.state  = CopySubstate::DistanceMantissaNibbles(
                                        len_decoded + 4,
                                        next_len_remaining,
                                        next_decoded_so_far);
                                }
                                return need_something;
                            },
                        }
                        len_decoded += 4;
                        decoded_so_far = next_decoded_so_far;
                    }
                    self.cc.distance = decoded_so_far;
                    self.state = CopySubstate::FullyDecoded;
                    return DivansResult::Success;
                },
                CopySubstate::FullyDecoded => {
                    return DivansResult::Success;
                }
            }
        }
    }
}
