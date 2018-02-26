use core;
use alloc::Allocator;
use brotli::BrotliResult;
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    Fail,
    round_up_mod_4,
    BLOCK_TYPE_DISTANCE_SWITCH,
    BLOCK_TYPE_COMMAND_SWITCH,
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
    DistanceLengthMnemonicTwo, // references a recent distance cached value
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
    fn transition_to_done<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                        Specialization:EncoderOrDecoderSpecialization,
                        Cdf16:CDF16,
                        AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<CDF2>,
                        AllocCDF16:Allocator<Cdf16>>(&mut self,
                                               superstate: &mut CrossCommandState<ArithmeticCoder,
                                                                                  Specialization,
                                                                                  Cdf16,
                                                                                  AllocU8,
                                                                                  AllocCDF2,
                                                                                  AllocCDF16>) {
        superstate.bk.btype_lru[BLOCK_TYPE_COMMAND_SWITCH][0].dec(1);
        superstate.bk.btype_lru[BLOCK_TYPE_DISTANCE_SWITCH][0].dec(1);
        self.state = CopySubstate::FullyDecoded;        
    }
    #[inline(always)]
    pub fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                        Specialization:EncoderOrDecoderSpecialization,
                        Cdf16:CDF16,
                        AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<CDF2>,
                        AllocCDF16:Allocator<Cdf16>>(&mut self,
                                               superstate: &mut CrossCommandState<ArithmeticCoder,
                                                                                  Specialization,
                                                                                  Cdf16,
                                                                                  AllocU8,
                                                                                  AllocCDF2,
                                                                                  AllocCDF16>,
                                               in_cmd: &CopyCommand,
                                               input_bytes:&[u8],
                                                    input_offset: &mut usize,
                                                    output_bytes:&mut [u8],
                                                    output_offset: &mut usize) -> BrotliResult {
        let dlen: u8 = (core::mem::size_of_val(&in_cmd.distance) as u32 * 8 - in_cmd.distance.leading_zeros()) as u8;
        let clen: u8 = (core::mem::size_of_val(&in_cmd.num_bytes) as u32 * 8 - in_cmd.num_bytes.leading_zeros()) as u8;
        if dlen ==0 {
            return Fail(); // not allowed to copy from 0 distance
        }
        loop {
            match superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset) {
                BrotliResult::ResultSuccess => {},
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
                    let index = ((superstate.bk.last_4_states as usize >> 4) & 3) + 4 * core::cmp::min(superstate.bk.last_llen as usize - 1, 3);
                    let ctype = superstate.bk.get_command_block_type();
                    let mut shortcut_nib = core::cmp::min(15, in_cmd.num_bytes) as u8;
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountSmall, (ctype, index));
                    superstate.coder.get_or_put_nibble(&mut shortcut_nib, nibble_prob, billing);
                    nibble_prob.blend(shortcut_nib, Speed::MED);

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
                    superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
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
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
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
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    let next_decoded_so_far = decoded_so_far | (u32::from(last_nib) << next_len_remaining);
                    nibble_prob.blend(last_nib, if index > 1 {Speed::MED} else {Speed::SLOW});

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
                        superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                        nibble_prob.blend(beg_nib, Speed::SLOW);
                    }
                    //println_stderr!("D {},{} => {} as {}", dtype, distance_map_index, actual_prior, beg_nib);
                    if beg_nib == 15 {
                        self.state = CopySubstate::DistanceLengthFirst;
                    } else {
                        self.cc.distance = superstate.bk.get_distance_from_mnemonic_code(beg_nib);
                        superstate.bk.last_dlen = (core::mem::size_of_val(&self.cc.distance) as u32 * 8
                                                   - self.cc.distance.leading_zeros()) as u8;
                        self.transition_to_done(superstate);
                    }
                },
                CopySubstate::DistanceLengthMnemonicTwo => {
                    //UNUSED : haven't made this pay for itself
                    let mut beg_nib = superstate.bk.distance_mnemonic_code_two(in_cmd.distance, in_cmd.num_bytes);
                    let actual_prior = superstate.bk.get_distance_prior(self.cc.num_bytes);
                    {
                        let mut nibble_prob = superstate.bk.copy_priors.get(
                            CopyCommandNibblePriorType::DistanceMnemonicTwo, (actual_prior as usize,));
                        superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                        nibble_prob.blend(beg_nib, Speed::MED);
                    }
                    if beg_nib == 15 {
                        self.state = CopySubstate::DistanceLengthFirst;
                    } else {
                        self.cc.distance = superstate.bk.get_distance_from_mnemonic_code_two(beg_nib,
                                                                                             self.cc.num_bytes);
                        superstate.bk.last_dlen = (core::mem::size_of_val(&self.cc.distance) as u32 * 8
                                                   - self.cc.distance.leading_zeros()) as u8;
                        self.transition_to_done(superstate);
                    }
                },
                CopySubstate::DistanceLengthFirst => {
                    let mut beg_nib = core::cmp::min(15, dlen - 1);
                    let index = (core::mem::size_of_val(&self.cc.num_bytes) as u32 * 8 - self.cc.num_bytes.leading_zeros()) as usize >> 2;
                    let actual_prior = superstate.bk.get_distance_prior(self.cc.num_bytes);
                    {
                        let mut nibble_prob = superstate.bk.copy_priors.get(
                            CopyCommandNibblePriorType::DistanceBegNib, (actual_prior as usize, index));
                        superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                        nibble_prob.blend(beg_nib, Speed::SLOW);
                    }
                    if beg_nib == 15 {
                        self.state = CopySubstate::DistanceLengthGreater15Less25;
                    } else {
                        superstate.bk.last_dlen = beg_nib + 1;
                        if beg_nib == 0 {
                            self.cc.distance = 1;
                            self.transition_to_done(superstate);
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
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::ROCKET);
                    superstate.bk.last_dlen = (last_nib + 15) + 1;
                    self.state = CopySubstate::DistanceMantissaNibbles(0, round_up_mod_4(last_nib + 15), 1 << (last_nib + 15));
                },
                CopySubstate::DistanceMantissaNibbles(len_decoded, len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = (in_cmd.distance ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    let index = if len_decoded == 0 { ((superstate.bk.last_dlen % 4) + 1) as usize } else { 0usize };
                    let actual_prior = superstate.bk.get_distance_prior(self.cc.num_bytes);
                    let next_decoded_so_far;
                    {
                        let mut nibble_prob = superstate.bk.copy_priors.get(
                            CopyCommandNibblePriorType::DistanceMantissaNib, (actual_prior, index));
                        superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                        next_decoded_so_far = decoded_so_far | (u32::from(last_nib) << next_len_remaining);
                        nibble_prob.blend(last_nib, if index > 1 {Speed::FAST} else {Speed::GLACIAL});
                    }
                    if next_len_remaining == 0 {
                        //println_stderr!("C:{}:D:{}", self.cc.num_bytes, next_decoded_so_far);
                        self.cc.distance = next_decoded_so_far;
                        self.transition_to_done(superstate);
                    } else {
                        self.state  = CopySubstate::DistanceMantissaNibbles(
                            len_decoded + 4,
                            next_len_remaining,
                            next_decoded_so_far);
                    }
                },
                CopySubstate::FullyDecoded => {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
}
