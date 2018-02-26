use core;
use brotli::BrotliResult;
use alloc::Allocator;
use brotli::transform::TransformDictionaryWord;
use ::priors::PriorCollection;
use brotli::dictionary::{kBrotliMaxDictionaryWordLength, kBrotliDictionary};
use ::probability::{CDF2, CDF16, Speed};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    round_up_mod_4,
    BLOCK_TYPE_DISTANCE_SWITCH,
    BLOCK_TYPE_COMMAND_SWITCH,
};
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    DictCommand,
};
use super::priors::{DictCommandNibblePriorType};


#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DictSubstate {
    Begin,
    WordSizeFirst,
    WordSizeGreater18Less25, // if in this state, second nibble results in values 19-24 (first nibble was between 4 and 18)
    WordIndexMantissa(u8, u8, u32), // assume the length is < (1 << WordSize), decode that many nibbles and use binary encoding
    TransformHigh, // total number of transforms <= 121 therefore; nibble must be < 8
    TransformLow,
    FullyDecoded,
}
pub struct DictState {
   pub dc:DictCommand,
   pub state: DictSubstate,
}
const DICT_BITS:[u8;25] = [
    0,  0,  0,  0, 10, 10, 11, 11, 10, 10,
    10, 10, 10,  9,  9,  8,  7,  7,  8,  7,
    7,  6,  6,  5,  5];


impl DictState {
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
                                                                                          AllocCDF16>,
    ) {
        self.state = DictSubstate::FullyDecoded;
        superstate.bk.btype_lru[BLOCK_TYPE_COMMAND_SWITCH][0].dec(1);
        superstate.bk.btype_lru[BLOCK_TYPE_DISTANCE_SWITCH][0].dec(1);
    }
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
                                               in_cmd: &DictCommand,
                                               input_bytes:&[u8],
                                               input_offset: &mut usize,
                                               output_bytes:&mut [u8],
                                               output_offset: &mut usize) -> BrotliResult {

        loop {
            match superstate.coder.drain_or_fill_internal_buffer(input_bytes, input_offset, output_bytes, output_offset) {
                BrotliResult::ResultSuccess => {},
                need_something => return need_something,
            }
            let billing = BillingDesignation::DictCommand(match self.state {
                DictSubstate::WordIndexMantissa(_, _, _) => DictSubstate::WordIndexMantissa(0, 0, 0),
                _ => self.state
            });

            match self.state {
                DictSubstate::Begin => {
                    self.state = DictSubstate::WordSizeFirst;
                },
                DictSubstate::WordSizeFirst => {
                    let mut beg_nib = core::cmp::min(15, in_cmd.word_size.wrapping_sub(4));
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.dict_priors.get(DictCommandNibblePriorType::SizeBegNib, (ctype,));
                    superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    nibble_prob.blend(beg_nib, Speed::MUD);

                    if beg_nib == 15 {
                        self.state = DictSubstate::WordSizeGreater18Less25;
                    } else {
                        self.dc.word_size = beg_nib + 4;
                        self.state = DictSubstate::WordIndexMantissa(0, round_up_mod_4(DICT_BITS[self.dc.word_size as usize]), 0);
                    }
                }
                DictSubstate::WordSizeGreater18Less25 => {
                    let mut beg_nib = in_cmd.word_size.wrapping_sub(19);
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.dict_priors.get(DictCommandNibblePriorType::SizeLastNib, (ctype,));
                    superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    nibble_prob.blend(beg_nib, Speed::MUD);

                    self.dc.word_size = beg_nib + 19;
                    if self.dc.word_size > 24 {
                        return BrotliResult::ResultFailure;
                    }
                    self.state = DictSubstate::WordIndexMantissa(0, round_up_mod_4(DICT_BITS[self.dc.word_size as usize]), 0);
                }
                DictSubstate::WordIndexMantissa(len_decoded, len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = (in_cmd.word_id ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    let index = if len_decoded == 0 { ((DICT_BITS[self.dc.word_size as usize] % 4) + 1) as usize } else { 0usize };
                    let actual_prior = superstate.bk.get_distance_prior(u32::from(self.dc.word_size));
                    let mut nibble_prob = superstate.bk.dict_priors.get(
                        DictCommandNibblePriorType::Index, (actual_prior, index));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::MUD);

                    let next_decoded_so_far = decoded_so_far | (u32::from(last_nib) << next_len_remaining);
                    if next_len_remaining == 0 {
                        self.dc.word_id = next_decoded_so_far;
                        self.state = DictSubstate::TransformHigh;
                    } else {
                        self.state  = DictSubstate::WordIndexMantissa(
                            len_decoded + 4,
                            next_len_remaining,
                            next_decoded_so_far);
                    }
                },
                DictSubstate::TransformHigh => {
                    let mut high_nib = in_cmd.transform >> 4;
                    let mut nibble_prob = superstate.bk.dict_priors.get(DictCommandNibblePriorType::Transform,
                                                                        (0, self.dc.word_size as usize >> 1));
                    superstate.coder.get_or_put_nibble(&mut high_nib, nibble_prob, billing);
                    nibble_prob.blend(high_nib, Speed::FAST);
                    self.dc.transform = high_nib << 4;
                    self.state = DictSubstate::TransformLow;
                }
                DictSubstate::TransformLow => {
                    let mut low_nib = in_cmd.transform & 0xf;
                    {
                        let mut nibble_prob = superstate.bk.dict_priors.get(DictCommandNibblePriorType::Transform,
                                                                            (1, self.dc.transform as usize >> 4));
                        superstate.coder.get_or_put_nibble(&mut low_nib, nibble_prob, billing);
                        nibble_prob.blend(low_nib, Speed::FAST);
                        self.dc.transform |= low_nib;
                        let dict = &kBrotliDictionary;
                        let word = &dict[(self.dc.word_id as usize)..(self.dc.word_id as usize + self.dc.word_size as usize)];
                        let mut transformed_word = [0u8;kBrotliMaxDictionaryWordLength as usize + 13];
                        let final_len = TransformDictionaryWord(&mut transformed_word[..],
                                                                &word[..],
                                                                i32::from(self.dc.word_size),
                                                                i32::from(self.dc.transform));
                        self.dc.final_size = final_len as u8;// WHA
                    }
                    self.transition_to_done(superstate);
                    return BrotliResult::ResultSuccess;
                }
                DictSubstate::FullyDecoded => {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
}
