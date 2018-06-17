use core;
use alloc::Allocator;
use interface::{DivansResult, ErrMsg, StreamMuxer, StreamDemuxer};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    round_up_mod_4,
    get_distance_from_mnemonic_code,
};
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    CopyCommand,
};
use ::priors::PriorCollection;
use ::probability::{Speed, CDF16};
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
    DistanceIsShortRepeat, // references a recent distance cached value
    DistanceLengthFirst,
    DistanceLengthGreater14Less25(u8), // lower 4 bits of dist_slot
    DistanceMantissaNibbles(u32, u8, u8, u32), // nibble count (up to 6), intermediate result
    FullyDecoded,
}
pub struct CopyState {
   pub cc:CopyCommand,
   pub state: CopySubstate,
   pub early_mnemonic: u8,
}

#[inline(always)]
fn get_dist_slot(dist_p_1: u32) -> u32 {
    fn get_dist_slot2(dist:u32) -> u32 {
        let i = 31 - dist.leading_zeros();
        (i + i) + ((dist >> (i - 1)) & 1)
    }
    let dist = dist_p_1 - 1;
    if dist <= 4 {
        dist
    } else {
        get_dist_slot2(dist)
    }
}


impl CopyState {
    pub fn begin() -> Self {
        CopyState{
            cc: CopyCommand {
                distance:0,
                num_bytes:0,
            },
            state:CopySubstate::Begin,
            early_mnemonic:0xfe,
        }
    }
    #[cfg_attr(not(feature="no-inline"), inline(always))]
    pub fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                            Specialization:EncoderOrDecoderSpecialization,
                            LinearInputBytes:StreamDemuxer<AllocU8>,
                            LinearOutputBytes:StreamMuxer<AllocU8>+Default,
                             
                        Cdf16:CDF16,
                        AllocU8:Allocator<u8>,
                        AllocCDF16:Allocator<Cdf16>>(&mut self,
                                                     superstate: &mut CrossCommandState<ArithmeticCoder,
                                                                                        Specialization,
                                                                                        LinearInputBytes,
                                                                                        LinearOutputBytes,
                                                                                        Cdf16,
                                                                                        AllocU8,
                                                                                        AllocCDF16>,
                                                     in_cmd: &CopyCommand,
                                                     output_bytes:&mut [u8],
                                                     output_offset: &mut usize) -> DivansResult {
        let dlen: u8 = (core::mem::size_of_val(&in_cmd.distance) as u32 * 8 - in_cmd.distance.leading_zeros()) as u8;
        let clen: u8 = (core::mem::size_of_val(&in_cmd.num_bytes) as u32 * 8 - in_cmd.num_bytes.leading_zeros()) as u8;
        let i_dist_slot = get_dist_slot(in_cmd.distance);
        if dlen ==0 {
            return DivansResult::Failure(ErrMsg::Distance0NotAllowed); // not allowed to copy from 0 distance
        }
        loop {
            match superstate.drain_or_fill_internal_buffer_cmd(output_bytes, output_offset) {
                DivansResult::Success => {},
                need_something => return need_something,
            }
            let billing = BillingDesignation::CopyCommand(match self.state {
                CopySubstate::CountMantissaNibbles(_, _, _) => CopySubstate::CountMantissaNibbles(0, 0, 0),
                CopySubstate::DistanceMantissaNibbles(_, _, _, _) => CopySubstate::DistanceMantissaNibbles(0, 0, 0, 0),
                CopySubstate::DistanceLengthGreater14Less25(_) => CopySubstate::DistanceLengthGreater14Less25(0),
                _ => self.state
            });
            match self.state {
                CopySubstate::Begin => {
                    if self.early_mnemonic != 0xff {
                        self.state = CopySubstate::DistanceLengthMnemonic;
                    } else {
                        self.state = CopySubstate::CountSmall;
                    }
                        
                },
                CopySubstate::CountSmall => {
                    // FIXME: this should somehow lean on the state_summary according to lzma
                    let index =  (superstate.bk.byte_index as usize&3);//((superstate.bk.last_4_states >> 4) & 3) as usize + 4 * core::cmp::min(superstate.bk.last_llen - 1, 3) as usize;
                    let mut shortcut_nib = if self.early_mnemonic == 0x0 && in_cmd.num_bytes == 1 {3} else if in_cmd.num_bytes >= 18 {2} else {(in_cmd.num_bytes > 9) as u8};
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountSmall, (ctype, index));
                    superstate.coder.get_or_put_nibble(&mut shortcut_nib, nibble_prob, billing);
                    if superstate.specialization.adapt_cdf() {
                        nibble_prob.blend(shortcut_nib, Speed::new(512,16384));
                    }

                    if shortcut_nib == 0 {
                        self.state = CopySubstate::CountLengthFirst;
                    } else if shortcut_nib == 1 {
                        self.state = CopySubstate::CountLengthGreater18Less25;
                    } else if shortcut_nib == 3 {
                        let (dist, ok, _cache_index) = get_distance_from_mnemonic_code(&superstate.bk.distance_lru,self.early_mnemonic, 1);
                        self.cc.distance = dist;
                        self.cc.num_bytes = 1;
                        superstate.bk.state_summary.obs_short_rep();
                        superstate.bk.last_dlen = get_dist_slot(self.cc.distance) as u8;
                        superstate.bk.last_clen = (core::mem::size_of_val(&self.cc.num_bytes) as u32 * 8
                                                   - (self.cc.num_bytes).leading_zeros()) as u8;
                        self.state = CopySubstate::FullyDecoded;
                        unreachable!();// no longer an active codepath
                    } else {
                        self.state = CopySubstate::CountMantissaNibbles(0, 8, 0);
                    }
                },
                CopySubstate::CountLengthFirst => {
                    let index = superstate.bk.byte_index as usize&3;//((superstate.bk.last_4_states >> 4) & 3) as usize + 4 * core::cmp::min(superstate.bk.last_llen - 1, 3) as usize;
                    let mut shortcut_nib = core::cmp::min(10, in_cmd.num_bytes) as u8;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountBegNib, (ctype, index));
                    superstate.coder.get_or_put_nibble(&mut shortcut_nib, nibble_prob, billing);
                    if superstate.specialization.adapt_cdf() {
                        nibble_prob.blend(shortcut_nib, Speed::new(128,16384));
                    }

                    if shortcut_nib == 10 {
                        self.state = CopySubstate::CountLengthGreater18Less25;
                        unreachable!();
                    } else {
                        self.cc.num_bytes = u32::from(shortcut_nib);
                        superstate.bk.last_clen = (core::mem::size_of_val(&self.cc.num_bytes) as u32 * 8
                                                   - (self.cc.num_bytes).leading_zeros()) as u8;
                        self.state = CopySubstate::CountDecoded;
                    }
                },
                CopySubstate::CountLengthGreater18Less25 => {
                    // at this point, num_bytes is at least 15, so clen is at least 4.
                    let mut beg_nib = core::cmp::min(10, in_cmd.num_bytes.wrapping_sub(9)) as u8;
                    let index = superstate.bk.byte_index as usize &3;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountLastNib, (ctype, index));
                    superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    if superstate.specialization.adapt_cdf() {
                        nibble_prob.blend(beg_nib, Speed::new(512,16384));
                    }
                    if beg_nib == 10 {
                        self.state = CopySubstate::CountMantissaNibbles(0, 8, 0);
                        unreachable!();
                    } else {
                        self.cc.num_bytes = u32::from(beg_nib) + 9;
                        superstate.bk.last_clen = (core::mem::size_of_val(&self.cc.num_bytes) as u32 * 8
                                                   - (self.cc.num_bytes).leading_zeros()) as u8;
                        self.state = CopySubstate::CountDecoded;
                    }
                },
                CopySubstate::CountMantissaNibbles(len_decoded, len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = ((in_cmd.num_bytes.wrapping_sub(18)) ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    let index = superstate.bk.byte_index as usize &3;//if len_decoded == 0 { ((superstate.bk.last_clen % 4) + 1) as usize } else { 0usize };
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountMantissaNib, (ctype,(len_decoded == 0) as usize * 0x40 +  index  * 0x10 + (0xf & (decoded_so_far as usize >> 4))));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    let next_decoded_so_far = decoded_so_far | (u32::from(last_nib) << next_len_remaining);
                    if superstate.specialization.adapt_cdf() {
                        nibble_prob.blend(last_nib, Speed::SLOW);
                    }

                    if next_len_remaining == 0 {
                        self.cc.num_bytes = next_decoded_so_far + 18;
                        self.state = CopySubstate::CountDecoded;
                    } else {
                        self.state  = CopySubstate::CountMantissaNibbles(
                            len_decoded + 4,
                            next_len_remaining,
                            next_decoded_so_far);
                    }
                },
                CopySubstate::CountDecoded => {
                    if self.early_mnemonic == 0xff {
                        self.state = CopySubstate::DistanceLengthMnemonic;
                    } else if self.early_mnemonic == 0xf {
                        superstate.bk.state_summary.obs_match();
                        self.state = CopySubstate::DistanceLengthFirst;
                    } else {
                        superstate.bk.state_summary.obs_long_rep();
                        let (dist, ok, _cache_index) = get_distance_from_mnemonic_code(&superstate.bk.distance_lru, self.early_mnemonic, self.cc.num_bytes);
                        self.cc.distance = dist;
                        superstate.bk.last_dlen = get_dist_slot(self.cc.distance) as u8;
                        if !ok {
                            return DivansResult::Failure(ErrMsg::CopyDistanceMnemonicCodeBad(dist as u8, (dist >> 8) as u8));
                        }
                        self.state = CopySubstate::FullyDecoded;                        
                    }
                },
                CopySubstate::DistanceLengthMnemonic => {
                    let mut beg_nib = if Specialization::IS_DECODING_FILE {
                        15 // we can't search for mnemonic in empty in_cmd (not yet decoded)
                    } else {
                        superstate.bk.distance_mnemonic_code(in_cmd.distance, self.cc.num_bytes)
                    };
                    if self.early_mnemonic == 0xfe {
                        assert_eq!(self.cc.num_bytes, 0);
                        if beg_nib >= 4 {
                            beg_nib = 15;
                        }
                    }
                    //let index = 0;
                    let actual_prior = usize::from(superstate.bk.state_summary as u8);//superstate.bk.get_distance_prior(self.cc.num_bytes);
                    {
                        let mut nibble_prob = superstate.bk.copy_priors.get(
                            CopyCommandNibblePriorType::DistanceMnemonic, (actual_prior as usize, superstate.bk.byte_index as usize&3/*((superstate.bk.last_llen < 8) as usize))*/));
                        superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                        if superstate.specialization.adapt_cdf() {
                            nibble_prob.blend(beg_nib, Speed::new(128,16384));
                        }
                    }
                    //println_stderr!("D {},{} => {} as {}", dtype, distance_map_index, actual_prior, beg_nib);
                    //eprintln!("M:{}", beg_nib);
                    if self.early_mnemonic == 0xfe {
                        if beg_nib == 0 {
                            self.early_mnemonic = beg_nib;
                            self.state = CopySubstate::DistanceIsShortRepeat;                            
                        } else if beg_nib == 4 {
                            // early exit
                            let (dist, ok, _cache_index) = get_distance_from_mnemonic_code(&superstate.bk.distance_lru,0, 1);
                            self.cc.distance = dist;
                            self.cc.num_bytes = 1;
                            superstate.bk.state_summary.obs_short_rep();
                            superstate.bk.last_dlen = get_dist_slot(self.cc.distance) as u8;
                            superstate.bk.last_clen = (core::mem::size_of_val(&self.cc.num_bytes) as u32 * 8
                                                       - (self.cc.num_bytes).leading_zeros()) as u8;
                            self.state = CopySubstate::FullyDecoded;
                        } else {
                            self.early_mnemonic = beg_nib;
                            self.state = CopySubstate::CountSmall;
                        }
                    }else if beg_nib == 15 {
                        superstate.bk.state_summary.obs_match();
                        self.state = CopySubstate::DistanceLengthFirst;
                    } else {
                        let (dist, ok, _cache_index) = get_distance_from_mnemonic_code(&superstate.bk.distance_lru, beg_nib, self.cc.num_bytes);
                        self.cc.distance = dist;
                        superstate.bk.last_dlen = get_dist_slot(dist) as u8;
                        if !ok {
                            return DivansResult::Failure(ErrMsg::CopyDistanceMnemonicCodeBad(dist as u8, (dist >> 8) as u8));
                        }
                        self.state = CopySubstate::FullyDecoded;
                    }
                },
                CopySubstate::DistanceIsShortRepeat => {
                    let mut beg_nib = (in_cmd.num_bytes == 1) as u8;
                    let actual_prior = usize::from(superstate.bk.state_summary as u8);//superstate.bk.get_distance_prior(self.cc.num_bytes);
                    {
                        let mut nibble_prob = superstate.bk.copy_priors.get(
                            CopyCommandNibblePriorType::DistanceShortRep, (actual_prior as usize, superstate.bk.byte_index as usize&3/*((superstate.bk.last_llen < 8) as usize))*/));
                        superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                        if superstate.specialization.adapt_cdf() {
                            nibble_prob.blend(beg_nib, Speed::new(512,16384));
                        }
                    }
                    if beg_nib == 1 {
                        let (dist, ok, _cache_index) = get_distance_from_mnemonic_code(&superstate.bk.distance_lru,0, 1);
                        self.cc.distance = dist;
                        self.cc.num_bytes = 1;
                        superstate.bk.state_summary.obs_short_rep();
                        superstate.bk.last_dlen = get_dist_slot(self.cc.distance) as u8;
                        superstate.bk.last_clen = (core::mem::size_of_val(&self.cc.num_bytes) as u32 * 8
                                                       - (self.cc.num_bytes).leading_zeros()) as u8;
                        self.state = CopySubstate::FullyDecoded;
                    } else {
                        self.state = CopySubstate::CountSmall;
                    }
                }
                CopySubstate::DistanceLengthFirst => {
                    let mut beg_nib = (i_dist_slot >> 4) as u8;
                    let index = core::cmp::min(self.cc.num_bytes as usize, 5);
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::DistanceBegNib, (0, index));
                    superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    if superstate.specialization.adapt_cdf() {
                        nibble_prob.blend(beg_nib, Speed::ROCKET);
                    }
                    self.state = CopySubstate::DistanceLengthGreater14Less25(beg_nib);
                },
                CopySubstate::DistanceLengthGreater14Less25(upper_slot) => {
                    let mut last_nib = i_dist_slot as u8& 0xf;
                    let index = core::cmp::min(self.cc.num_bytes as usize, 5);
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::DistanceLastNib, (upper_slot as usize, index));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    if superstate.specialization.adapt_cdf() {
                        nibble_prob.blend(last_nib, Speed::new(32,4096));
                    }
                    let o_dist_slot = (upper_slot << 4) | last_nib;
                    superstate.bk.last_dlen = o_dist_slot as u8;
                    if superstate.bk.last_dlen < 4 {
                        self.cc.distance = 1 + o_dist_slot as u32;
                        self.state = CopySubstate::FullyDecoded;
                    } else {
                        let dist_slot = o_dist_slot as u32;
                        let footer_bits = (dist_slot >> 1).wrapping_sub(1);
                        let base1 = (2 | (dist_slot  & 1)) << footer_bits;
                        let footer_p1_bits = ((dist_slot + 1) >> 1).wrapping_sub(1);
                        let base2 = (2 | ((dist_slot +1) & 1)) << footer_p1_bits;
                        let logrange = 32 - ((base2 - base1 - 1) as u32).leading_zeros();
                        //eprintln!("Computing slot {} ranges from {} - {}. Num bits {}", dist_slot, base1, base2, logrange);
                        self.state = CopySubstate::DistanceMantissaNibbles(o_dist_slot as u32, 0, round_up_mod_4(logrange as u8), 0);
                    }
                },
                CopySubstate::DistanceMantissaNibbles(dist_slot, mut len_decoded, start_len_remaining, mut decoded_so_far) => {
                    let footer_bits = (dist_slot >> 1).wrapping_sub(1);
                    let base = (2 | (dist_slot  & 1)) << footer_bits;
                    let dist_reduced = in_cmd.distance.wrapping_sub(base + 1);
                    for next_len_remaining_sr2 in (0..((start_len_remaining as usize + 3) >> 2)).rev() {
                        let next_len_remaining = (next_len_remaining_sr2 as u8) << 2;
                        let actual_prior = superstate.bk.get_distance_prior(self.cc.num_bytes);
                        let last_nib_as_u32 = (dist_reduced ^ decoded_so_far) >> next_len_remaining;
                        let mut last_nib = last_nib_as_u32 as u8;
                        let index = if len_decoded == 0 { ((superstate.bk.last_dlen & 3) + 1) as usize } else { 0usize };
                        let four_if_0_or_1_64_if_2_3_or_4 = 0x4 << ((index & 6) << ((index & 2)>>1));
                        let next_decoded_so_far;
                        let speed;
                        {
                            let billing;
                            let mut nibble_prob = if dist_slot < 14 {
                                billing = CopySubstate::DistanceMantissaNibbles(0, 0, 0, 0);
                                speed = 512;
                                superstate.bk.copy_priors.get(
                                    CopyCommandNibblePriorType::DistanceMantissaNib, ( (base - dist_slot + decoded_so_far) as usize, next_len_remaining_sr2))
                            } else {
                                if next_len_remaining_sr2 == 0 {
                                    billing = CopySubstate::DistanceMantissaNibbles(1, 0, 0, 0);
                                    speed = 128;
                                    superstate.bk.copy_priors.get(
                                        CopyCommandNibblePriorType::DistanceAlignNib, (0,))
                                } else{
                                    billing = CopySubstate::DistanceMantissaNibbles(2, 0, 0, 0);
                                    speed = if len_decoded != 0 {
                                        128
                                    } else {
                                        128
                                    };
                                    superstate.bk.copy_priors.get(
                                        CopyCommandNibblePriorType::DistanceDirectNib, (dist_slot as usize - 14, ((len_decoded + 3) >>2) as usize))
                                }
                            };
                                superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, BillingDesignation::CopyCommand(
                                billing));
                            next_decoded_so_far = decoded_so_far | (u32::from(last_nib) << next_len_remaining);
                            if superstate.specialization.adapt_cdf() {
                                nibble_prob.blend(last_nib, Speed::new(speed, 16384));
                            }
                        }
                        match superstate.drain_or_fill_internal_buffer_cmd(output_bytes, output_offset) {
                            DivansResult::Success => {},
                            need_something => {
                                if next_len_remaining == 0 {
                                    self.cc.distance = next_decoded_so_far + base + 1;
                                    self.state = CopySubstate::FullyDecoded;
                                } else {
                                    self.state  = CopySubstate::DistanceMantissaNibbles(
                                        dist_slot,
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
                    self.cc.distance = decoded_so_far + base + 1;
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
