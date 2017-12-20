use core;
use brotli::BrotliResult;
use ::probability::{CDF2, CDF16, Speed};
use super::priors::RandLiteralNibblePriorType;
use ::slice_util::AllocatedMemoryPrefix;
use ::alloc_util::UninitializedOnAlloc;
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    round_up_mod_4,
};
use super::specializations::CodecTraits;
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    RandLiteralCommand,
};
use ::priors::PriorCollection;
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RandLiteralSubstate {
    Begin,
    LiteralCountSmall,
    LiteralCountFirst,
    LiteralCountLengthGreater14Less25,
    LiteralCountMantissaNibbles(u8, u32),
    LiteralNibbleIndex(u32),
    LiteralNibbleLowerHalf(u32),
    FullyDecoded,
}

pub struct RandLiteralState<AllocU8:Allocator<u8>> {
    pub lc:RandLiteralCommand<AllocatedMemoryPrefix<u8, AllocU8>>,
    pub state: RandLiteralSubstate,
}


impl<AllocU8:Allocator<u8>,
                         > RandLiteralState<AllocU8> {
    #[inline(always)]
    pub fn code_nibble<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                       Cdf16:CDF16,
                       Specialization:EncoderOrDecoderSpecialization,
                       AllocCDF2:Allocator<CDF2>,
                       AllocCDF16:Allocator<Cdf16>,
                       CTraits:CodecTraits,
                       >(&mut self,
                         mut cur_nibble: u8,
                         superstate: &mut CrossCommandState<ArithmeticCoder,
                                                            Specialization,
                                                            Cdf16,
                                                            AllocU8,
                                                            AllocCDF2,
                                                            AllocCDF16>) -> u8 {
        let _range = superstate.coder.get_or_put_nibble(
            &mut cur_nibble,
            &Cdf16::default(),
            BillingDesignation::RandLiteralCommand(
                RandLiteralSubstate::LiteralNibbleIndex(0)));
        cur_nibble
    }
    pub fn encode_or_decode<ISlice: SliceWrapper<u8>,
                            ArithmeticCoder:ArithmeticEncoderOrDecoder,
                            Cdf16:CDF16,
                            Specialization:EncoderOrDecoderSpecialization,
                            AllocCDF2:Allocator<CDF2>,
                            AllocCDF16:Allocator<Cdf16>,
                        >(&mut self,
                          superstate: &mut CrossCommandState<ArithmeticCoder,
                                                             Specialization,
                                                             Cdf16,
                                                             AllocU8,
                                                             AllocCDF2,
                                                             AllocCDF16>,
                          in_cmd: &RandLiteralCommand<ISlice>,
                          input_bytes:&[u8],
                          input_offset: &mut usize,
                          output_bytes:&mut [u8],
                          output_offset: &mut usize) -> BrotliResult {
        let literal_len = in_cmd.data.slice().len() as u32;
        let serialized_large_literal_len  = literal_len.wrapping_sub(16);
        let lllen: u8 = (core::mem::size_of_val(&serialized_large_literal_len) as u32 * 8 - serialized_large_literal_len.leading_zeros()) as u8;
        loop {
            match superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset) {
                BrotliResult::ResultSuccess => {},
                need_something => return need_something,
            }
            let billing = BillingDesignation::RandLiteralCommand(match self.state {
                RandLiteralSubstate::LiteralCountMantissaNibbles(_, _) => RandLiteralSubstate::LiteralCountMantissaNibbles(0, 0),
                _ => self.state
            });
            match self.state {
                RandLiteralSubstate::Begin => {
                    self.state = RandLiteralSubstate::LiteralCountSmall;
                },
                RandLiteralSubstate::LiteralCountSmall => {
                    let index = 0;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut shortcut_nib = core::cmp::min(15, literal_len.wrapping_sub(1)) as u8;
                    let mut nibble_prob = superstate.bk.lit_priors.get(
                        RandLiteralNibblePriorType::CountSmall, (ctype, index));
                    superstate.coder.get_or_put_nibble(&mut shortcut_nib, nibble_prob, billing);
                    nibble_prob.blend(shortcut_nib, Speed::MED);// checked med

                    if shortcut_nib == 15 {
                        self.state = RandLiteralSubstate::LiteralCountFirst;
                    } else {
                        self.lc.data = superstate.m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(shortcut_nib as usize + 1);
                        self.state = self.get_nibble_code_state(0, in_cmd);
                    }
                },
                RandLiteralSubstate::LiteralCountFirst => {
                    let mut beg_nib = core::cmp::min(15, lllen);
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.lit_priors.get(RandLiteralNibblePriorType::SizeBegNib, (ctype,));
                    superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    nibble_prob.blend(beg_nib, Speed::MUD);

                    if beg_nib == 15 {
                        self.state = RandLiteralSubstate::LiteralCountLengthGreater14Less25;
                    } else if beg_nib <= 1 {
                        self.lc.data = superstate.m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(16 + beg_nib as usize);
                        self.state = self.get_nibble_code_state(0, in_cmd);
                    } else {
                        self.state = RandLiteralSubstate::LiteralCountMantissaNibbles(round_up_mod_4(beg_nib - 1),
                                                                                  1 << (beg_nib - 1));
                    }
                },
                RandLiteralSubstate::LiteralCountLengthGreater14Less25 => {
                    let mut last_nib = lllen.wrapping_sub(15);
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.lit_priors.get(RandLiteralNibblePriorType::SizeLastNib, (ctype,));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::MUD);

                    self.state = RandLiteralSubstate::LiteralCountMantissaNibbles(
                        round_up_mod_4(last_nib + 14),
                        1 << (last_nib + 14));
                },
                RandLiteralSubstate::LiteralCountMantissaNibbles(len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = (serialized_large_literal_len ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.lit_priors.get(RandLiteralNibblePriorType::SizeMantissaNib, (ctype,));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::MUD);
                    let next_decoded_so_far = decoded_so_far | (u32::from(last_nib) << next_len_remaining);

                    if next_len_remaining == 0 {
                        self.lc.data = superstate.m8.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(next_decoded_so_far as usize + 16);
                        self.state = self.get_nibble_code_state(0, in_cmd);
                    } else {
                        self.state  = RandLiteralSubstate::LiteralCountMantissaNibbles(
                            next_len_remaining,
                            next_decoded_so_far);
                    }
                },
                RandLiteralSubstate::LiteralNibbleLowerHalf(nibble_index) => {
                    assert_eq!(nibble_index & 1, 1); // this is only for odd nibbles
                    let byte_index = (nibble_index as usize) >> 1;
                    let mut byte_to_encode_val = superstate.specialization.get_literal_byte(in_cmd, byte_index);
                    {
                        let cur_nibble = self.code_nibble(byte_to_encode_val & 0xf,
                                                          superstate,
                                                          );
                        let cur_byte = &mut self.lc.data.slice_mut()[byte_index];
                        *cur_byte = cur_nibble | *cur_byte;
                        superstate.bk.push_literal_byte(*cur_byte);
                    }
                    if byte_index + 1 == self.lc.data.slice().len() {
                        self.state = RandLiteralSubstate::FullyDecoded;
                        return BrotliResult::ResultSuccess;
                    } else {
                        self.state = RandLiteralSubstate::LiteralNibbleIndex(nibble_index + 1);
                    }
                },
                RandLiteralSubstate::LiteralNibbleIndex(nibble_index) => {
                    superstate.bk.last_llen = self.lc.data.slice().len() as u32;
                    let byte_index = (nibble_index as usize) >> 1;
                    let mut byte_to_encode_val = superstate.specialization.get_literal_byte(in_cmd, byte_index);
                    let high_nibble = self.code_nibble(byte_to_encode_val >> 4, superstate);
                    match superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset) {
                        BrotliResult::ResultSuccess => {},
                        need_something => {
                            return self.fallback_byte_encode(high_nibble, nibble_index, need_something);
                        }
                    }
                    let low_nibble = self.code_nibble(byte_to_encode_val & 0xf, superstate);
                    let cur_byte = (high_nibble << 4) | low_nibble;
                    self.lc.data.slice_mut()[byte_index] = cur_byte;
                    superstate.bk.push_literal_byte(cur_byte);
                    if byte_index + 1 == self.lc.data.slice().len() {
                        self.state = RandLiteralSubstate::FullyDecoded;
                        return BrotliResult::ResultSuccess;
                    } else {
                        self.state = RandLiteralSubstate::LiteralNibbleIndex(nibble_index + 2);
                    }
                },
                RandLiteralSubstate::FullyDecoded => {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
    #[cold]
    fn fallback_byte_encode(&mut self, cur_nibble: u8, nibble_index: u32, res: BrotliResult) -> BrotliResult{
        self.lc.data.slice_mut()[(nibble_index >> 1) as usize] = cur_nibble << 4;
        self.state = RandLiteralSubstate::LiteralNibbleLowerHalf(nibble_index + 1);
        res
    }
}
