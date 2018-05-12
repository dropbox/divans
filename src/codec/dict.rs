use core;
use interface::{DivansResult, ErrMsg, StreamMuxer, StreamDemuxer};
use alloc::Allocator;
use brotli::transform::TransformDictionaryWord;
use brotli::interface::Nop;
use ::priors::PriorCollection;
use brotli::dictionary::{kBrotliMaxDictionaryWordLength, kBrotliDictionary};
use ::probability::{CDF16, Speed};
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    round_up_mod_4,
    CMD_CODER,
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
    pub fn begin() -> Self {
        DictState {
            dc: DictCommand::nop(),
            state: DictSubstate::Begin,
        }
    }
    pub fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                        Specialization:EncoderOrDecoderSpecialization,
                        Cdf16:CDF16,
                        LinearInputBytes:StreamDemuxer<AllocU8>+Default,
                        LinearOutputBytes:StreamMuxer<AllocU8>+Default,                             
                        AllocU8:Allocator<u8>,
                        AllocCDF16:Allocator<Cdf16>>(&mut self,
                                               superstate: &mut CrossCommandState<ArithmeticCoder,
                                                                                  Specialization,
                                                                                  LinearInputBytes,
                                                                                  LinearOutputBytes,
                                                                                  Cdf16,
                                                                                  AllocU8,
                                                                                  AllocCDF16>,
                                               in_cmd: &DictCommand,
                                               output_bytes:&mut [u8],
                                               output_offset: &mut usize) -> DivansResult {

        loop {
            match superstate.drain_or_fill_internal_buffer(CMD_CODER, output_bytes, output_offset) {
                DivansResult::Success => {},
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
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
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
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    nibble_prob.blend(beg_nib, Speed::MUD);

                    self.dc.word_size = beg_nib + 19;
                    if self.dc.word_size > 24 {
                        return DivansResult::Failure(ErrMsg::DictWordSizeTooLarge(self.dc.word_size as u8));
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
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut last_nib, nibble_prob, billing);
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
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut high_nib, nibble_prob, billing);
                    nibble_prob.blend(high_nib, Speed::FAST);
                    self.dc.transform = high_nib << 4;
                    self.state = DictSubstate::TransformLow;
                }
                DictSubstate::TransformLow => {
                    let mut low_nib = in_cmd.transform & 0xf;
                    let mut nibble_prob = superstate.bk.dict_priors.get(DictCommandNibblePriorType::Transform,
                                                                        (1, self.dc.transform as usize >> 4));
                    superstate.coder[CMD_CODER].get_or_put_nibble(&mut low_nib, nibble_prob, billing);
                    nibble_prob.blend(low_nib, Speed::FAST);
                    self.dc.transform |= low_nib;
                    let dict = &kBrotliDictionary;
                    let word = &dict[(self.dc.word_id as usize)..(self.dc.word_id as usize + self.dc.word_size as usize)];
                    let mut transformed_word = [0u8;kBrotliMaxDictionaryWordLength as usize + 13];
                    if self.dc.transform >= 121 {
                        return DivansResult::Failure(ErrMsg::DictTransformIndexUndefined(self.dc.transform));
                    }
                    let final_len = TransformDictionaryWord(&mut transformed_word[..],
                                                            &word[..],
                                                            i32::from(self.dc.word_size),
                                                            i32::from(self.dc.transform));
                    self.dc.final_size = final_len as u8;// WHA
                    self.state = DictSubstate::FullyDecoded;
                    return DivansResult::Success;
                }
                DictSubstate::FullyDecoded => {
                    return DivansResult::Success;
                }
            }
        }
    }
}
