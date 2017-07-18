#![allow(dead_code)]
use core;
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use brotli_decompressor::dictionary::{kBrotliMaxDictionaryWordLength, kBrotliDictionary};
use brotli_decompressor::BrotliResult;
pub const CMD_BUFFER_SIZE: usize = 16;
use brotli_decompressor::transform::{TransformDictionaryWord};
use priors::{PriorCollection, PriorMultiIndex};

use interface::{
    BillingDesignation,
    CrossCommandBilling,
    BlockSwitch,
    Nop
};
/*
use std::io::Write;
macro_rules! println_stderr(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);
*/
use super::probability::{BaseCDF, CDF2, CDF16, Speed};
use super::interface::{
    ArithmeticEncoderOrDecoder,
    Command,
    CopyCommand,
    DictCommand,
    LiteralCommand,
};

pub struct AllocatedMemoryPrefix<AllocU8:Allocator<u8>>(AllocU8::AllocatedMemory, usize);

pub trait EncoderOrDecoderSpecialization {
    fn alloc_literal_buffer<AllocU8: Allocator<u8>>(&mut self,
                                                    m8: &mut AllocU8,
                                                    len: usize) -> AllocatedMemoryPrefix<AllocU8>;
    fn get_input_command<'a, ISlice:SliceWrapper<u8>>(&self, data:&'a [Command<ISlice>],offset: usize,
                                                      backing:&'a Command<ISlice>) -> &'a Command<ISlice>;
    fn get_output_command<'a, AllocU8:Allocator<u8>>(&self, data:&'a mut [Command<AllocatedMemoryPrefix<AllocU8>>],
                                                     offset: usize,
                                                     backing:&'a mut Command<AllocatedMemoryPrefix<AllocU8>>) -> &'a mut Command<AllocatedMemoryPrefix<AllocU8>>;
    fn get_source_copy_command<'a, ISlice:SliceWrapper<u8>>(&self, &'a Command<ISlice>, &'a CopyCommand) -> &'a CopyCommand;
    fn get_source_literal_command<'a, ISlice:SliceWrapper<u8>+Default>(&self, &'a Command<ISlice>, &'a LiteralCommand<ISlice>) -> &'a LiteralCommand<ISlice>;
    fn get_source_dict_command<'a, ISlice:SliceWrapper<u8>>(&self, &'a Command<ISlice>, &'a DictCommand) -> &'a DictCommand;
    fn get_literal_byte<ISlice:SliceWrapper<u8>>(&self,
                                                   in_cmd: &LiteralCommand<ISlice>,
                                                   index: usize) -> u8;
    fn get_recoder_output<'a>(&'a mut self, passed_in_output_bytes: &'a mut [u8]) -> &'a mut[u8];
    fn get_recoder_output_offset<'a>(&self,
                                     passed_in_output_bytes: &'a mut usize,
                                     backing: &'a mut usize) -> &'a mut usize;
    fn does_caller_want_original_file_bytes(&self) -> bool;
}



impl<AllocU8:Allocator<u8>> Default for AllocatedMemoryPrefix<AllocU8> {
    fn default() -> Self {
        AllocatedMemoryPrefix(AllocU8::AllocatedMemory::default(), 0usize)
    }
}
impl<AllocU8:Allocator<u8>> AllocatedMemoryPrefix<AllocU8> {
    fn replace_with_empty(&mut self) ->AllocU8::AllocatedMemory {
        core::mem::replace(&mut self.0, AllocU8::AllocatedMemory::default())
    }
}

impl<AllocU8:Allocator<u8>> SliceWrapperMut<u8> for AllocatedMemoryPrefix<AllocU8> {
    fn slice_mut(&mut self) -> &mut [u8] {
        self.0.slice_mut().split_at_mut(self.1).0
    }
}
impl<AllocU8:Allocator<u8>> SliceWrapper<u8> for AllocatedMemoryPrefix<AllocU8> {
    fn slice(&self) -> &[u8] {
        self.0.slice().split_at(self.1).0
    }
}
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
struct CopyState {
   cc:CopyCommand,
   state: CopySubstate,
}

fn round_up_mod_4(val: u8) -> u8 {
    ((val - 1)|3)+1
}

fn round_up_mod_4_u32(val: u32) -> u32 {
    ((val - 1)|3)+1
}


#[allow(non_snake_case)]
fn Fail() -> BrotliResult {
    BrotliResult::ResultFailure
}

impl CopyState {
    fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
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
                    let index = 0;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut shortcut_nib = core::cmp::min(15, in_cmd.num_bytes.wrapping_sub(2)) as u8;
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountSmall, (index, ctype));
                    superstate.coder.get_or_put_nibble(&mut shortcut_nib, nibble_prob, billing);
                    nibble_prob.blend(shortcut_nib, Speed::SLOW);

                    if shortcut_nib == 15 {
                        self.state = CopySubstate::CountLengthFirst;
                    } else {
                        self.cc.num_bytes = (shortcut_nib + 2) as u32;
                        superstate.bk.last_clen = (core::mem::size_of_val(&self.cc.num_bytes) as u32 * 8
                                                   - (self.cc.num_bytes).leading_zeros()) as u8;
                        self.state = CopySubstate::CountDecoded;
                    }
                },
                CopySubstate::CountLengthFirst => {
                    let mut beg_nib = core::cmp::min(15, clen.wrapping_sub(4));
                    let index = 0;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountBegNib, (index, ctype));
                    superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    nibble_prob.blend(beg_nib, Speed::FAST);

                    if beg_nib == 15 {
                        self.state = CopySubstate::CountLengthGreater18Less25;
                    } else {
                        superstate.bk.last_clen = beg_nib + 4;
                        self.state = CopySubstate::CountMantissaNibbles(0, round_up_mod_4(beg_nib - 1 + 4), 1 << (beg_nib - 1 + 4));
                    }
                },
                CopySubstate::CountLengthGreater18Less25 => {
                    let mut last_nib = clen.wrapping_sub(19);
                    let index = 0;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountLastNib, (index, ctype));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::FAST);
                    superstate.bk.last_clen = last_nib + 15;
                    self.state = CopySubstate::CountMantissaNibbles(0, round_up_mod_4(last_nib + 14), 1 << (last_nib + 14));
                },
                CopySubstate::CountMantissaNibbles(len_decoded, len_remaining, decoded_so_far) => {
                    let next_len_remaining = len_remaining - 4;
                    let last_nib_as_u32 = (in_cmd.num_bytes ^ decoded_so_far) >> next_len_remaining;
                    // debug_assert!(last_nib_as_u32 < 16); only for encoding
                    let mut last_nib = last_nib_as_u32 as u8;
                    let index = if len_decoded == 0 { ((superstate.bk.last_clen % 4) + 1) as usize } else { 0usize };
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::CountMantissaNib, (index, ctype));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    let next_decoded_so_far = decoded_so_far | ((last_nib as u32) << next_len_remaining);
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
                    let mut beg_nib = superstate.bk.distance_mnemonic_code(in_cmd.distance);
                    //let index = 0;
                    let dtype = superstate.bk.get_distance_block_type();
                    {
                        let mut nibble_prob = superstate.bk.copy_priors.get(
                            CopyCommandNibblePriorType::DistanceMnemonic,
                            dtype);
                        superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                        nibble_prob.blend(beg_nib, Speed::MUD);
                    }
                    if beg_nib == 15 {
                        self.state = CopySubstate::DistanceLengthFirst;
                    } else {
                        self.cc.distance = superstate.bk.get_distance_from_mnemonic_code(beg_nib);
                        superstate.bk.last_dlen = (core::mem::size_of_val(&self.cc.distance) as u32 * 8
                                                   - self.cc.distance.leading_zeros()) as u8;
                        self.state = CopySubstate::FullyDecoded;
                    }
                },
                CopySubstate::DistanceLengthMnemonicTwo => {
                    //UNUSED : haven't made this pay for itself
                    let mut beg_nib = superstate.bk.distance_mnemonic_code_two(in_cmd.distance, in_cmd.num_bytes);
                    let dtype = superstate.bk.get_distance_block_type();
                    {
                        let mut nibble_prob = superstate.bk.copy_priors.get(
                            CopyCommandNibblePriorType::DistanceMnemonicTwo,
                            dtype);
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
                        self.state = CopySubstate::FullyDecoded;
                    }
                },
                CopySubstate::DistanceLengthFirst => {
                    let mut beg_nib = core::cmp::min(15, dlen - 1);
                    let index = (core::mem::size_of_val(&self.cc.num_bytes) as u32 * 8 - self.cc.num_bytes.leading_zeros()) as usize >> 2;
                    let dtype = superstate.bk.get_distance_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::DistanceBegNib, (index, dtype));
                    superstate.coder.get_or_put_nibble(&mut beg_nib, nibble_prob, billing);
                    nibble_prob.blend(beg_nib, Speed::PLANE);
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
                    let dtype = superstate.bk.get_distance_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::DistanceLastNib, (index, dtype));
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
                    let dtype = superstate.bk.get_distance_block_type();
                    let mut nibble_prob = superstate.bk.copy_priors.get(
                        CopyCommandNibblePriorType::DistanceMantissaNib, (index, dtype));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    let next_decoded_so_far = decoded_so_far | ((last_nib as u32) << next_len_remaining);
                    nibble_prob.blend(last_nib, if index > 1 {Speed::FAST} else {Speed::GLACIAL});

                    if next_len_remaining == 0 {
                        //println_stderr!("C:{}:D:{}", self.cc.num_bytes, next_decoded_so_far);
                        self.cc.distance = next_decoded_so_far;
                        self.state = CopySubstate::FullyDecoded;
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

impl <AllocU8:Allocator<u8>> AllocatedMemoryPrefix<AllocU8> {
    pub fn new(m8 : &mut AllocU8, len: usize) -> Self {
        AllocatedMemoryPrefix::<AllocU8>(m8.alloc_cell(len), len)
    }
}

impl<AllocU8:Allocator<u8>> From<CopyState> for Command<AllocatedMemoryPrefix<AllocU8>> {
     fn from(cp: CopyState) -> Self {
        Command::Copy(cp.cc)
     }
}
impl<AllocU8:Allocator<u8>> From<DictState> for Command<AllocatedMemoryPrefix<AllocU8>> {
     fn from(dd: DictState) -> Self {
        Command::Dict(dd.dc)
     }
}
impl<AllocU8:Allocator<u8>> From<LiteralState<AllocU8>> for Command<AllocatedMemoryPrefix<AllocU8>> {
     fn from(ll: LiteralState<AllocU8>) -> Self {
        Command::Literal(ll.lc)
     }
}

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
struct DictState {
   dc:DictCommand,
   state: DictSubstate,
}
const DICT_BITS:[u8;25] = [
    0,  0,  0,  0, 10, 10, 11, 11, 10, 10,
    10, 10, 10,  9,  9,  8,  7,  7,  8,  7,
    7,  6,  6,  5,  5];

impl DictState {
    fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
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
                    let mut nibble_prob = superstate.bk.dict_priors.get(DictCommandNibblePriorType::SizeBegNib,
                                                                        ctype);
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
                    let mut nibble_prob = superstate.bk.dict_priors.get(DictCommandNibblePriorType::SizeLastNib,
                                                                        ctype);
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
                    let dtype = superstate.bk.get_distance_block_type();
                    let mut nibble_prob = superstate.bk.dict_priors.get(
                        DictCommandNibblePriorType::Index, (index, dtype));
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::MUD);

                    let next_decoded_so_far = decoded_so_far | ((last_nib as u32) << next_len_remaining);
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
                    let mut nibble_prob = superstate.bk.dict_priors.get(DictCommandNibblePriorType::Transform, 0);
                    superstate.coder.get_or_put_nibble(&mut high_nib, nibble_prob, billing);
                    nibble_prob.blend(high_nib, Speed::FAST);
                    self.dc.transform = high_nib << 4;
                    self.state = DictSubstate::TransformLow;
                }
                DictSubstate::TransformLow => {
                    let mut low_nib = in_cmd.transform & 0xf;
                    let mut nibble_prob = superstate.bk.dict_priors.get(DictCommandNibblePriorType::Transform, 1 + (self.dc.transform as usize >>5));
                    superstate.coder.get_or_put_nibble(&mut low_nib, nibble_prob, billing);
                    nibble_prob.blend(low_nib, Speed::FAST);
                    self.dc.transform |= low_nib;
                    let dict = &kBrotliDictionary;
                    let word = &dict[(self.dc.word_id as usize)..(self.dc.word_id as usize + self.dc.word_size as usize)];
                    let mut transformed_word = [0u8;kBrotliMaxDictionaryWordLength as usize + 13];
                    let final_len = TransformDictionaryWord(&mut transformed_word[..],
                                                            &word[..],
                                                            self.dc.word_size as i32,
                                                            self.dc.transform as i32);
                    self.dc.final_size = final_len as u8;// WHA
                    self.state = DictSubstate::FullyDecoded;
                    return BrotliResult::ResultSuccess;
                }
                DictSubstate::FullyDecoded => {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
}

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
struct LiteralState<AllocU8:Allocator<u8>> {
    lc:LiteralCommand<AllocatedMemoryPrefix<AllocU8>>,
    state: LiteralSubstate,
}

impl<AllocU8:Allocator<u8>,
                         > LiteralState<AllocU8> {
    fn encode_or_decode<ISlice: SliceWrapper<u8>,
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
        let ltype = superstate.bk.get_literal_block_type();
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
                        LiteralNibblePriorType::CountSmall, (index, ctype));
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
                    let mut beg_nib = lllen;
                    let ctype = superstate.bk.get_command_block_type();
                    let mut nibble_prob = superstate.bk.lit_priors.get(LiteralNibblePriorType::SizeBegNib, ctype);
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
                    let mut nibble_prob = superstate.bk.lit_priors.get(LiteralNibblePriorType::SizeLastNib, ctype);
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
                    let mut nibble_prob = superstate.bk.lit_priors.get(LiteralNibblePriorType::SizeMantissaNib, ctype);
                    superstate.coder.get_or_put_nibble(&mut last_nib, nibble_prob, billing);
                    nibble_prob.blend(last_nib, Speed::MUD);
                    let next_decoded_so_far = decoded_so_far | ((last_nib as u32) << next_len_remaining);

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
                    superstate.bk.last_llen = self.lc.data.slice().len() as u8;
                    let byte_index = (nibble_index as usize) >> 1;
                    let high_nibble = (nibble_index & 1) == 0;
                    let shift : u8 = if high_nibble { 4 } else { 0 };
                    let mut cur_nibble = (superstate.specialization.get_literal_byte(in_cmd, byte_index)
                                          >> shift) & 0xf;
                    let k0 = ((superstate.bk.last_8_literals >> 0x3c) & 0xf) as usize;
                    let k1 = ((superstate.bk.last_8_literals >> 0x38) & 0xf) as usize;
                    let _k2 = ((superstate.bk.last_8_literals >> 0x34) & 0xf) as usize;
                    let _k3 = ((superstate.bk.last_8_literals >> 0x30) & 0xf) as usize;
                    let _k4 = ((superstate.bk.last_8_literals >> 0x2c) & 0xf) as usize;
                    let _k5 = ((superstate.bk.last_8_literals >> 0x28) & 0xf) as usize;
                    let _k6 = ((superstate.bk.last_8_literals >> 0x24) & 0xf) as usize;
                    let _k7 = ((superstate.bk.last_8_literals >> 0x20) & 0xf) as usize;
                    let _k8 = ((superstate.bk.last_8_literals >> 0x1c) & 0xf) as usize;
                    {
                        let nibble_index_truncated = if nibble_index < 2 { nibble_index } else { 2 };
                        let mut nibble_prob = superstate.bk.lit_priors.get(
                            if high_nibble { LiteralNibblePriorType::FirstNibble } else { LiteralNibblePriorType::SecondNibble },
                            (ltype, k0 as usize, k1 as usize, nibble_index_truncated as usize));
                        superstate.coder.get_or_put_nibble(&mut cur_nibble, nibble_prob, billing);
                        nibble_prob.blend(cur_nibble, Speed::SLOW);
                    }
                    self.lc.data.slice_mut()[byte_index] |= cur_nibble << shift;
                    superstate.bk.push_literal_nibble(cur_nibble);
                    /*
                    if (nibble_index & 1) == 1 {
                        println_stderr!("{}{}{}{}{}",
                                        ((k7<<4)|k8) as u8 as char,
                                        ((k5<<4)|k6) as u8 as char,
                                        ((k3<<4)|k4) as u8 as char,
                                        ((k1<<4)|k2) as u8 as char,
                                        self.lc.data.slice_mut()[byte_index] as char);
                    }
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

#[derive(Clone,Copy)]
enum BlockTypeState {
    Begin,
    TwoNibbleType,
    FinalNibble(u8),
    FullyDecoded(u8),
}
impl BlockTypeState {
    fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                        Specialization:EncoderOrDecoderSpecialization,
                        Cdf16:CDF16,
                        AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<CDF2>,
                        AllocCDF16:Allocator<Cdf16>>(
        &mut self,
        superstate: &mut CrossCommandState<ArithmeticCoder,
                                           Specialization,
                                           Cdf16,
                                           AllocU8,
                                           AllocCDF2,
                                           AllocCDF16>,
        input_bs: BlockSwitch,
        block_type_switch_index:usize,
        input_bytes: &[u8],
        input_offset: &mut usize,
        output_bytes: &mut [u8],
        output_offset: &mut usize) -> BrotliResult {
        let mut varint_nibble:u8 =
            if input_bs.block_type() == superstate.bk.btype_lru[block_type_switch_index][1] {
                0
            } else if input_bs.block_type() == superstate.bk.btype_lru[block_type_switch_index][0] + 1 {
                1
            } else if input_bs.block_type() <= 12 {
                input_bs.block_type() + 2
            } else {
                15
            };
        let mut first_nibble:u8 = input_bs.block_type() & 0xf;
        let mut second_nibble:u8 = input_bs.block_type() >> 4;
        loop {
            match superstate.coder.drain_or_fill_internal_buffer(input_bytes,
                                                                 input_offset,
                                                                 output_bytes,
                                                                 output_offset) {
                BrotliResult::ResultSuccess => {},
                need_something => return need_something,
            }
            match *self {
                BlockTypeState::Begin => {
                    superstate.coder.get_or_put_nibble(
                        &mut varint_nibble,
                        &superstate.bk.btype_prior[block_type_switch_index][0],
                        BillingDesignation::CrossCommand(CrossCommandBilling::BlockSwitchType));
                    superstate.bk.btype_prior[block_type_switch_index][0].blend(varint_nibble, Speed::SLOW);
                    match varint_nibble {
                        0 => *self = BlockTypeState::FullyDecoded(
                            superstate.bk.btype_lru[block_type_switch_index][1]),
                        1 => *self = BlockTypeState::FullyDecoded(
                            superstate.bk.btype_lru[block_type_switch_index][0] + 1),
                        15 => *self = BlockTypeState::TwoNibbleType,
                        val => *self = BlockTypeState::FullyDecoded(val - 2),
                    }
                },
                BlockTypeState::TwoNibbleType => {
                    superstate.coder.get_or_put_nibble(
                        &mut first_nibble,
                        &superstate.bk.btype_prior[block_type_switch_index][1],
                        BillingDesignation::CrossCommand(CrossCommandBilling::BlockSwitchType));
                    superstate.bk.btype_prior[block_type_switch_index][1].blend(first_nibble, Speed::SLOW);
                    *self = BlockTypeState::FinalNibble(first_nibble);
                },
                BlockTypeState::FinalNibble(first_nibble) => {
                    superstate.coder.get_or_put_nibble(
                        &mut second_nibble,
                        &superstate.bk.btype_prior[block_type_switch_index][2],
                        BillingDesignation::CrossCommand(CrossCommandBilling::BlockSwitchType));
                    superstate.bk.btype_prior[block_type_switch_index][2].blend(second_nibble, Speed::SLOW);
                    *self = BlockTypeState::FinalNibble((second_nibble << 4) | first_nibble);
                }
                BlockTypeState::FullyDecoded(_) =>   {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
}
enum EncodeOrDecodeState<AllocU8: Allocator<u8> > {
    Begin,
    Literal(LiteralState<AllocU8>),
    Dict(DictState),
    Copy(CopyState),
    BlockSwitchLiteral(BlockTypeState),
    BlockSwitchCommand(BlockTypeState),
    BlockSwitchDistance(BlockTypeState),
    PopulateRingBuffer(Command<AllocatedMemoryPrefix<AllocU8>>),
    DivansSuccess,
    EncodedShutdownNode, // in flush/close state (encoder only) and finished flushing the EOF node type
    ShutdownCoder,
    CoderBufferDrain,
    WriteChecksum(usize),
}

const CHECKSUM_LENGTH: usize = 8;


impl<AllocU8:Allocator<u8>> Default for EncodeOrDecodeState<AllocU8> {
    fn default() -> Self {
        EncodeOrDecodeState::Begin
    }
}
const NUM_BLOCK_TYPES:usize = 256;
const LOG_NUM_COPY_TYPE_PRIORS: usize = 2;
const LOG_NUM_DICT_TYPE_PRIORS: usize = 2;
const BLOCK_TYPE_LITERAL_SWITCH:usize=0;
const BLOCK_TYPE_COMMAND_SWITCH:usize=0;
const BLOCK_TYPE_DISTANCE_SWITCH:usize=0;
define_prior_struct!(CrossCommandPriors, CrossCommandBilling,
                     (CrossCommandBilling::FullSelection, 4, NUM_BLOCK_TYPES),
                     (CrossCommandBilling::CopyIndicator, 4, NUM_BLOCK_TYPES),
                     (CrossCommandBilling::DictIndicator, 4, NUM_BLOCK_TYPES),
                     (CrossCommandBilling::EndIndicator, 1, NUM_BLOCK_TYPES));

#[derive(PartialEq, Debug, Clone)]
enum LiteralNibblePriorType {
    FirstNibble,
    SecondNibble,
    CountSmall,
    SizeBegNib,
    SizeLastNib,
    SizeMantissaNib,
}

define_prior_struct!(LiteralCommandPriors, LiteralNibblePriorType,
                     (LiteralNibblePriorType::FirstNibble, NUM_BLOCK_TYPES, 16, 16, 3),
                     (LiteralNibblePriorType::SecondNibble, NUM_BLOCK_TYPES, 16, 16, 3),
                     (LiteralNibblePriorType::CountSmall, 16, NUM_BLOCK_TYPES),
                     (LiteralNibblePriorType::SizeBegNib, 1, NUM_BLOCK_TYPES),
                     (LiteralNibblePriorType::SizeLastNib, 1, NUM_BLOCK_TYPES),
                     (LiteralNibblePriorType::SizeMantissaNib, 1, NUM_BLOCK_TYPES));

#[derive(PartialEq, Debug, Clone)]
enum CopyCommandNibblePriorType {
    DistanceBegNib,
    DistanceLastNib,
    DistanceMnemonic,
    DistanceMnemonicTwo,
    DistanceMantissaNib,
    CountSmall,
    CountBegNib,
    CountLastNib,
    CountMantissaNib,
}
const NUM_COPY_COMMAND_ORGANIC_PRIORS: usize = 64;
define_prior_struct!(CopyCommandPriors, CopyCommandNibblePriorType,
                     (CopyCommandNibblePriorType::DistanceBegNib, NUM_COPY_COMMAND_ORGANIC_PRIORS, NUM_BLOCK_TYPES),
                     (CopyCommandNibblePriorType::DistanceMnemonic, 1, NUM_BLOCK_TYPES),
                     (CopyCommandNibblePriorType::DistanceLastNib, NUM_COPY_COMMAND_ORGANIC_PRIORS, NUM_BLOCK_TYPES),
                     (CopyCommandNibblePriorType::DistanceMantissaNib, NUM_COPY_COMMAND_ORGANIC_PRIORS, NUM_BLOCK_TYPES),
                     (CopyCommandNibblePriorType::CountSmall, NUM_COPY_COMMAND_ORGANIC_PRIORS, NUM_BLOCK_TYPES),
                     (CopyCommandNibblePriorType::CountBegNib, NUM_COPY_COMMAND_ORGANIC_PRIORS, NUM_BLOCK_TYPES),
                     (CopyCommandNibblePriorType::CountLastNib, NUM_COPY_COMMAND_ORGANIC_PRIORS, NUM_BLOCK_TYPES),
                     (CopyCommandNibblePriorType::CountMantissaNib, NUM_COPY_COMMAND_ORGANIC_PRIORS, NUM_BLOCK_TYPES));

#[derive(PartialEq, Debug, Clone)]
enum DictCommandNibblePriorType {
    SizeBegNib,
    SizeLastNib,
    Index,
    Transform,
}

const NUM_ORGANIC_DICT_DISTANCE_PRIORS: usize = 5;
define_prior_struct!(DictCommandPriors, DictCommandNibblePriorType,
                     (DictCommandNibblePriorType::SizeBegNib, 1, NUM_BLOCK_TYPES),
                     (DictCommandNibblePriorType::SizeLastNib, 1, NUM_BLOCK_TYPES),
                     (DictCommandNibblePriorType::Index, NUM_ORGANIC_DICT_DISTANCE_PRIORS, NUM_BLOCK_TYPES),
                     (DictCommandNibblePriorType::Transform, 17));

#[derive(Copy,Clone)]
pub struct DistanceCacheEntry {
    distance:u32,
    decode_byte_count:u32,
}

pub struct CrossCommandBookKeeping<Cdf16:CDF16,
                                   AllocCDF2:Allocator<CDF2>,
                                   AllocCDF16:Allocator<Cdf16>> {
    decode_byte_count: u32,
    command_count:u32,
    last_8_literals: u64,
    last_4_states: u8,
    last_dlen: u8,
    last_clen: u8,
    last_llen: u8,
    lit_priors: LiteralCommandPriors<Cdf16, AllocCDF16>,
    cc_priors: CrossCommandPriors<Cdf16, AllocCDF16>,
    copy_priors: CopyCommandPriors<Cdf16, AllocCDF16>,
    dict_priors: DictCommandPriors<Cdf16, AllocCDF16>,
    distance_lru: [u32;4],
    btype_prior: [[Cdf16;3];3],
    btype_lru: [[u8;2];3],
    distance_cache:[[DistanceCacheEntry;3];32],
    _legacy: core::marker::PhantomData<AllocCDF2>,
}


fn sub_or_add(val: u32, sub: u32, add: u32) -> u32 {
    if val >= sub {
        val - sub
    } else {
        val + add
    }
}

impl<Cdf16:CDF16,
     AllocCDF2:Allocator<CDF2>,
     AllocCDF16:Allocator<Cdf16>> CrossCommandBookKeeping<Cdf16,
                                                          AllocCDF2,
                                                          AllocCDF16> {
    fn new(lit_prior: AllocCDF16::AllocatedMemory,
           cc_prior: AllocCDF16::AllocatedMemory,
           copy_prior: AllocCDF16::AllocatedMemory,
           dict_prior: AllocCDF16::AllocatedMemory) -> Self {
        let mut ret = CrossCommandBookKeeping{
            decode_byte_count:0,
            command_count:0,
            distance_cache:[
                [
                    DistanceCacheEntry{
                        distance:1,
                        decode_byte_count:0,
                    };3];32],
            last_dlen: 1,
            last_llen: 1,
            last_clen: 1,
            last_4_states: 0,
            last_8_literals: 0,
            lit_priors: LiteralCommandPriors {
                priors: lit_prior
            },
            cc_priors: CrossCommandPriors {
                priors: cc_prior
            },
            copy_priors: CopyCommandPriors {
                priors: copy_prior
            },
            dict_priors: DictCommandPriors {
                priors: dict_prior,
            },
            btype_prior: [[Cdf16::default(),
                           Cdf16::default(),
                           Cdf16::default()];3],
            distance_lru: [4,11,15,16],
            btype_lru:[[0,1];3],
            _legacy: core::marker::PhantomData::<AllocCDF2>::default(),
        };
        for i in 0..4 {
            for j in 0..0x10 {
                let prob = ret.cc_priors.get(CrossCommandBilling::FullSelection,
                                             (i, j));
                for _samp in 0..1 {
                    prob.blend(0x1, Speed::FAST);
                    prob.blend(0x1, Speed::FAST);
                    prob.blend(0x2, Speed::FAST);
                    prob.blend(0x1, Speed::FAST);
                    prob.blend(0x1, Speed::FAST);
                    prob.blend(0x1, Speed::FAST);
                    prob.blend(0x2, Speed::FAST);
                    prob.blend(0x3, Speed::FAST);
                    prob.blend(0x3, Speed::FAST);
                }
            }
        }
        ret
    }
    fn read_distance_cache(&self, len:u32, index:u32) -> u32 {
        let len_index = core::cmp::min(len as usize, self.distance_cache.len() - 1);
        return self.distance_cache[len_index][index as usize].distance + (
            self.decode_byte_count - self.distance_cache[len_index][index as usize].decode_byte_count);
    }
    fn get_distance_from_mnemonic_code_two(&self, code:u8, len:u32,) -> u32 {
        match code {
            0 => sub_or_add(self.distance_lru[2], 1, 3),
            1 => self.read_distance_cache(len, 0),
            2 => self.read_distance_cache(len, 1),
            3 => self.read_distance_cache(len, 2),
            4 => self.read_distance_cache(len + 1, 0),
            5 => self.read_distance_cache(len + 1, 1),
            6 => self.read_distance_cache(len + 1, 2),
            7 => self.read_distance_cache(len + 1, 0) - 1,
            8 => self.read_distance_cache(len + 1, 1) - 1,
            9 => self.read_distance_cache(len + 1, 2) - 1,
            10 => self.read_distance_cache(len + 2, 0),
            11 => self.read_distance_cache(len + 2, 1),
            12 => self.read_distance_cache(len + 2, 2),
            13 => self.read_distance_cache(len + 2, 0) - 1,
            14 => self.read_distance_cache(len + 2, 1) - 1,
            _ => panic!("Logic error: nibble > 14 evaluated for nmemonic"),
        }
    }
    fn distance_mnemonic_code_two(&self, d: u32, len:u32) -> u8 {
        for i in 0..15 {
            if self.get_distance_from_mnemonic_code_two(i as u8, len) == d {
                return i as u8;
            }
        }
        15
    }

    fn get_distance_from_mnemonic_code(&self, code:u8) -> u32 {
        match code {
            0 => self.distance_lru[0],
            1 => self.distance_lru[1],
            2 => self.distance_lru[2],
            3 => self.distance_lru[3],
            4 => self.distance_lru[0] + 1,
            5 => sub_or_add(self.distance_lru[0], 1, 4),
            6 => self.distance_lru[1] + 1,
            7 => sub_or_add(self.distance_lru[1], 1, 3),
            8 => self.distance_lru[0] + 2,
            9 => sub_or_add(self.distance_lru[0], 2, 5),
            10 => self.distance_lru[1] + 2,
            11 => sub_or_add(self.distance_lru[1], 2, 4),
            12 => self.distance_lru[0] + 3,
            13 => sub_or_add(self.distance_lru[0], 3, 6),
            14 => self.distance_lru[1] + 3,
            _ => panic!("Logic error: nibble > 14 evaluated for nmemonic"),
        }
    }
    fn distance_mnemonic_code(&self, d: u32) -> u8 {
        for i in 0..15 {
            if self.get_distance_from_mnemonic_code(i as u8) == d {
                return i as u8;
            }
        }
        15
    }
    fn get_command_block_type(&self) -> usize {
        self.btype_lru[BLOCK_TYPE_COMMAND_SWITCH][0] as usize
    }
    fn get_distance_block_type(&self) -> usize {
        self.btype_lru[BLOCK_TYPE_DISTANCE_SWITCH][0] as usize
    }
    fn get_literal_block_type(&self) -> usize {
        self.btype_lru[BLOCK_TYPE_LITERAL_SWITCH][0] as usize
    }
    fn push_literal_nibble(&mut self, nibble: u8) {
        self.last_8_literals >>= 0x4;
        self.last_8_literals |= (nibble as u64) << 0x3c;
    }
    fn get_command_type_prob<'a>(&'a mut self) -> &'a mut Cdf16 {
        //let last_8 = self.cross_command_state.recoder.last_8_literals();
        self.cc_priors.get(CrossCommandBilling::FullSelection,
                           ((self.last_4_states as usize) >> (8 - LOG_NUM_COPY_TYPE_PRIORS),
                           ((self.last_8_literals>>0x3e) as usize &0xf)))
    }
    fn next_state(&mut self) {
        self.last_4_states >>= 2;
    }
    fn obs_dict_state(&mut self) {
        self.next_state();
        self.last_4_states |= 192;
    }
    fn obs_copy_state(&mut self) {
        self.next_state();
        self.last_4_states |= 64;
    }
    fn obs_literal_state(&mut self) {
        self.next_state();
        self.last_4_states |= 128;
    }
    fn obs_distance(&mut self, cc:&CopyCommand) {
        if cc.num_bytes < self.distance_cache.len() as u32{
            let nb = cc.num_bytes as usize;
            let mut sub_index = 0usize;
            if self.distance_cache[nb][1].decode_byte_count < self.distance_cache[nb][0].decode_byte_count {
                sub_index = 1;
            }
            if self.distance_cache[nb][2].decode_byte_count < self.distance_cache[nb][sub_index].decode_byte_count {
                sub_index = 2;
            }
            self.distance_cache[nb][sub_index] = DistanceCacheEntry{
                distance: 0,//cc.distance, we're copying it to here (ha!)
                decode_byte_count:self.decode_byte_count,
            };
        }
        let distance = cc.distance;
        if distance == self.distance_lru[1] {
            self.distance_lru = [distance,
                                 self.distance_lru[0],
                                 self.distance_lru[2],
                                 self.distance_lru[3]];
        } else if distance == self.distance_lru[2] {
            self.distance_lru = [distance,
                                 self.distance_lru[0],
                                 self.distance_lru[1],
                                 self.distance_lru[3]];
        } else if distance != self.distance_lru[0] {
            self.distance_lru = [distance,
                                 self.distance_lru[0],
                                 self.distance_lru[1],
                                 self.distance_lru[2]];
        }
    }
    fn obs_btypel(&mut self, btype:u8) {
        self.btype_lru[BLOCK_TYPE_LITERAL_SWITCH] = [btype, self.btype_lru[BLOCK_TYPE_LITERAL_SWITCH][0]];
    }
    fn obs_btypec(&mut self, btype:u8) {
        self.btype_lru[BLOCK_TYPE_COMMAND_SWITCH] = [btype, self.btype_lru[BLOCK_TYPE_COMMAND_SWITCH][0]];
    }
    fn obs_btyped(&mut self, btype:u8) {
        self.btype_lru[BLOCK_TYPE_DISTANCE_SWITCH] = [btype, self.btype_lru[BLOCK_TYPE_DISTANCE_SWITCH][0]];
    }
}

pub struct CrossCommandState<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                             Specialization:EncoderOrDecoderSpecialization,
                             Cdf16:CDF16,
                             AllocU8:Allocator<u8>,
                             AllocCDF2:Allocator<CDF2>,
                             AllocCDF16:Allocator<Cdf16>> {
    coder: ArithmeticCoder,
    specialization: Specialization,
    recoder: super::cmd_to_raw::DivansRecodeState<AllocU8::AllocatedMemory>,
    m8: AllocU8,
    mcdf2: AllocCDF2,
    mcdf16: AllocCDF16,
    bk: CrossCommandBookKeeping<Cdf16, AllocCDF2, AllocCDF16>,
}

impl <ArithmeticCoder:ArithmeticEncoderOrDecoder,
      Specialization:EncoderOrDecoderSpecialization,
      Cdf16:CDF16,
                             AllocU8:Allocator<u8>,
                             AllocCDF2:Allocator<CDF2>,
                             AllocCDF16:Allocator<Cdf16>
      > CrossCommandState<ArithmeticCoder,
                          Specialization,
                          Cdf16,
                          AllocU8,
                          AllocCDF2,
                          AllocCDF16> {
    fn new(mut m8: AllocU8,
           mcdf2:AllocCDF2,
           mut mcdf16:AllocCDF16,
           coder: ArithmeticCoder,
           spc: Specialization, ring_buffer_size: usize) -> Self {
        let ring_buffer = m8.alloc_cell(1 << ring_buffer_size);
        let lit_priors = mcdf16.alloc_cell(LiteralCommandPriors::<Cdf16, AllocCDF16>::num_all_priors());
        let copy_priors = mcdf16.alloc_cell(CopyCommandPriors::<Cdf16, AllocCDF16>::num_all_priors());
        let dict_priors = mcdf16.alloc_cell(DictCommandPriors::<Cdf16, AllocCDF16>::num_all_priors());
        let cc_priors = mcdf16.alloc_cell(CrossCommandPriors::<Cdf16, AllocCDF16>::num_all_priors());
        CrossCommandState::<ArithmeticCoder,
                            Specialization,
                            Cdf16,
                            AllocU8,
                            AllocCDF2,
                            AllocCDF16> {
            coder: coder,
            specialization: spc,
            recoder: super::cmd_to_raw::DivansRecodeState::<AllocU8::AllocatedMemory>::new(
                ring_buffer),
            m8: m8,
            mcdf2:mcdf2,
            mcdf16:mcdf16,
            bk:CrossCommandBookKeeping::new(lit_priors, cc_priors, copy_priors, dict_priors),
        }
    }
    fn free(mut self) -> (AllocU8, AllocCDF2, AllocCDF16) {
        let rb = core::mem::replace(&mut self.recoder.ring_buffer, AllocU8::AllocatedMemory::default());
        let cdf16a = core::mem::replace(&mut self.bk.cc_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16b = core::mem::replace(&mut self.bk.copy_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16c = core::mem::replace(&mut self.bk.dict_priors.priors, AllocCDF16::AllocatedMemory::default());
        let cdf16d = core::mem::replace(&mut self.bk.lit_priors.priors, AllocCDF16::AllocatedMemory::default());
        self.m8.free_cell(rb);
        self.mcdf16.free_cell(cdf16a);
        self.mcdf16.free_cell(cdf16b);
        self.mcdf16.free_cell(cdf16c);
        self.mcdf16.free_cell(cdf16d);
        (self.m8, self.mcdf2, self.mcdf16)
    }
}

pub fn command_type_to_nibble<SliceType:SliceWrapper<u8>>(cmd:&Command<SliceType>,
                                                          is_end: bool) -> u8 {

    if is_end {
        return 0xf;
    }
    match cmd {
        &Command::Copy(_) => return 0x1,
        &Command::Dict(_) => return 0x2,
        &Command::Literal(_) => return 0x3,
        &Command::BlockSwitchCommand(_) => return 0x4,
        &Command::BlockSwitchLiteral(_) => return 0x5,
        &Command::BlockSwitchDistance(_) => return 0x6,
    }
}
#[cfg(feature="bitcmdselect")]
fn use_legacy_bitwise_command_type_code() -> bool {
    true
}
fn get_command_state_from_nibble<AllocU8:Allocator<u8>>(command_type_code:u8) -> EncodeOrDecodeState<AllocU8> {
   match command_type_code {
      1 => EncodeOrDecodeState::Copy(CopyState {
                            cc: CopyCommand {
                                distance:0,
                                num_bytes:0,
                            },
                            state:CopySubstate::Begin,
                        }),
      2 => EncodeOrDecodeState::Dict(DictState {
                                dc: DictCommand::nop(),
                                state: DictSubstate::Begin,
                            }),
      3 => EncodeOrDecodeState::Literal(LiteralState {
                                lc:LiteralCommand::<AllocatedMemoryPrefix<AllocU8>>{
                                    data:AllocatedMemoryPrefix::default(),
                                },
                                state:LiteralSubstate::Begin,
                            }),
     4 => EncodeOrDecodeState::BlockSwitchLiteral(BlockTypeState::Begin),
     5 => EncodeOrDecodeState::BlockSwitchCommand(BlockTypeState::Begin),
     6 => EncodeOrDecodeState::BlockSwitchDistance(BlockTypeState::Begin),
     0xf => EncodeOrDecodeState::DivansSuccess,
      _ => panic!("unimpl"),
   }
}

pub struct DivansCodec<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                       Specialization:EncoderOrDecoderSpecialization,
                       Cdf16:CDF16,
                       AllocU8: Allocator<u8>,
                       AllocCDF2:Allocator<CDF2>,
                       AllocCDF16:Allocator<Cdf16>> {
    cross_command_state: CrossCommandState<ArithmeticCoder,
                                           Specialization,
                                           Cdf16,
                                           AllocU8,
                                           AllocCDF2,
                                           AllocCDF16>,
    state : EncodeOrDecodeState<AllocU8>,
}

pub enum OneCommandReturn {
    Advance,
    BufferExhausted(BrotliResult),
}

impl<ArithmeticCoder:ArithmeticEncoderOrDecoder,
     Specialization: EncoderOrDecoderSpecialization,
     Cdf16:CDF16,
     AllocU8: Allocator<u8>,
     AllocCDF2: Allocator<CDF2>,
     AllocCDF16:Allocator<Cdf16>> DivansCodec<ArithmeticCoder, Specialization, Cdf16, AllocU8, AllocCDF2, AllocCDF16> {
    pub fn free(self) -> (AllocU8, AllocCDF2, AllocCDF16) {
        self.cross_command_state.free()
    }
    pub fn new(m8:AllocU8,
               mcdf2:AllocCDF2,
               mcdf16:AllocCDF16,
               coder: ArithmeticCoder,
               specialization: Specialization,
               ring_buffer_size: usize) -> Self {
        DivansCodec::<ArithmeticCoder,  Specialization, Cdf16, AllocU8, AllocCDF2, AllocCDF16> {
            cross_command_state:CrossCommandState::<ArithmeticCoder,
                                                    Specialization,
                                                    Cdf16,
                                                    AllocU8,
                                                    AllocCDF2,
                                                    AllocCDF16>::new(m8,
                                                                     mcdf2,
                                                                     mcdf16,
                                                                     coder,
                                                                     specialization,
                                                                     ring_buffer_size),
            state:EncodeOrDecodeState::Begin,
        }
    }
    pub fn get_coder(&self) -> &ArithmeticCoder {
        &self.cross_command_state.coder
    }
    pub fn specialization(&mut self) -> &mut Specialization{
        &mut self.cross_command_state.specialization
    }
    pub fn coder(&mut self) -> &mut ArithmeticCoder {
        &mut self.cross_command_state.coder
    }
    pub fn flush(&mut self,
                 output_bytes: &mut [u8],
                 output_bytes_offset: &mut usize) -> BrotliResult{
        //FIXME: track states here somehow must  map from Begin -> wherever we are
        loop {
            match self.state {
                EncodeOrDecodeState::Begin => {
                    let mut unused = 0usize;
                    let nop = Command::<AllocU8::AllocatedMemory>::nop();
                    match self.encode_or_decode_one_command(&[],
                                                            &mut unused,
                                                            output_bytes,
                                                            output_bytes_offset,
                                                            &nop,
                                                            true) {
                        OneCommandReturn::BufferExhausted(res) => {
                            match res {
                                BrotliResult::ResultSuccess => {},
                                need => return need,
                            }
                        },
                        OneCommandReturn::Advance => panic!("Unintended state: flush => Advance"),
                    }
                    self.state = EncodeOrDecodeState::EncodedShutdownNode;
                },
                EncodeOrDecodeState::EncodedShutdownNode => {
                    let mut unused = 0usize;
                    match self.cross_command_state.coder.drain_or_fill_internal_buffer(&[], &mut unused, output_bytes, output_bytes_offset) {
                        BrotliResult::ResultSuccess => self.state = EncodeOrDecodeState::ShutdownCoder,
                        ret => return ret,
                    }
                },
                EncodeOrDecodeState::ShutdownCoder => {
                    match self.cross_command_state.coder.close() {
                        BrotliResult::ResultSuccess => self.state = EncodeOrDecodeState::CoderBufferDrain,
                        ret => return ret,
                    }
                },
                EncodeOrDecodeState::CoderBufferDrain => {
                    let mut unused = 0usize;
                    match self.cross_command_state.coder.drain_or_fill_internal_buffer(&[],
                                                                                       &mut unused,
                                                                                       output_bytes,
                                                                                       output_bytes_offset) {
                        BrotliResult::ResultSuccess => {
                            self.state = EncodeOrDecodeState::WriteChecksum(0);
                        },
                        ret => return ret,
                    }
                },
                EncodeOrDecodeState::WriteChecksum(count) => {
                    let bytes_remaining = output_bytes.len() - *output_bytes_offset;
                    let bytes_needed = CHECKSUM_LENGTH - count;
                    let count_to_copy = core::cmp::min(bytes_remaining,
                                                       bytes_needed);
                    let checksum = ['~' as u8,
                                    'd' as u8,
                                    'i' as u8,
                                    'v' as u8,
                                    'a' as u8,
                                    'n' as u8,
                                    's' as u8,
                                    '~' as u8];
                    output_bytes.split_at_mut(*output_bytes_offset).1.split_at_mut(
                        count_to_copy).0.clone_from_slice(checksum.split_at(count_to_copy).0);
                    *output_bytes_offset += count_to_copy;
                    if bytes_needed <= bytes_remaining {
                        self.state = EncodeOrDecodeState::DivansSuccess;
                        return BrotliResult::ResultSuccess;
                    } else {
                        self.state = EncodeOrDecodeState::WriteChecksum(count + count_to_copy);
                        return BrotliResult::NeedsMoreOutput;
                    }
                },
                EncodeOrDecodeState::DivansSuccess => return BrotliResult::ResultSuccess,
                _ => return Fail(), // not allowed to flush if previous command was partially processed
            }
        }
    }
    pub fn encode_or_decode<ISl:SliceWrapper<u8>+Default>(&mut self,
                                                  input_bytes: &[u8],
                                                  input_bytes_offset: &mut usize,
                                                  output_bytes: &mut [u8],
                                                  output_bytes_offset: &mut usize,
                                                  input_commands: &[Command<ISl>],
                                                  input_command_offset: &mut usize) -> BrotliResult {
        loop {
            let i_cmd_backing = Command::<ISl>::nop();
            let in_cmd = self.cross_command_state.specialization.get_input_command(input_commands,
                                                                                   *input_command_offset,
                                                                                   &i_cmd_backing);
            match self.encode_or_decode_one_command(input_bytes,
                                                    input_bytes_offset,
                                                    output_bytes,
                                                    output_bytes_offset,
                                                    in_cmd,
                                                    false /* not end*/) {
                OneCommandReturn::Advance => {
                    *input_command_offset += 1;
                    if input_commands.len() == *input_command_offset {
                        return BrotliResult::NeedsMoreInput;
                    }
                },
                OneCommandReturn::BufferExhausted(result) => {
                    return result;
                }
            }
        }
    }
    pub fn encode_or_decode_one_command<ISl:SliceWrapper<u8>+Default>(&mut self,
                                                  input_bytes: &[u8],
                                                  input_bytes_offset: &mut usize,
                                                  output_bytes: &mut [u8],
                                                  output_bytes_offset: &mut usize,
                                                  input_cmd: &Command<ISl>,
                                                  is_end: bool) -> OneCommandReturn {
        loop {
            let new_state: Option<EncodeOrDecodeState<AllocU8>>;
            match &mut self.state {
                &mut EncodeOrDecodeState::EncodedShutdownNode
                    | &mut EncodeOrDecodeState::ShutdownCoder
                    | &mut EncodeOrDecodeState::CoderBufferDrain
                    | &mut EncodeOrDecodeState::WriteChecksum(_) => {
                    // not allowed to encode additional commands after flush is invoked
                    return OneCommandReturn::BufferExhausted(Fail());
                }
                &mut EncodeOrDecodeState::DivansSuccess => {
                    return OneCommandReturn::BufferExhausted(BrotliResult::ResultSuccess);
                },
                &mut EncodeOrDecodeState::Begin => {
                    match self.cross_command_state.coder.drain_or_fill_internal_buffer(input_bytes, input_bytes_offset,
                                                                                      output_bytes, output_bytes_offset) {
                        BrotliResult::ResultSuccess => {},
                        need_something => return OneCommandReturn::BufferExhausted(need_something),
                    }
                    let mut command_type_code = command_type_to_nibble(input_cmd, is_end);
                    {
                        let command_type_prob = self.cross_command_state.bk.get_command_type_prob();
                        self.cross_command_state.coder.get_or_put_nibble(
                            &mut command_type_code,
                            command_type_prob,
                            BillingDesignation::CrossCommand(CrossCommandBilling::FullSelection));
                        command_type_prob.blend(command_type_code, Speed::ROCKET);
                    }
                    let command_state = get_command_state_from_nibble(command_type_code);
                    match command_state {
                        EncodeOrDecodeState::Copy(_) => { self.cross_command_state.bk.obs_copy_state(); },
                        EncodeOrDecodeState::Dict(_) => { self.cross_command_state.bk.obs_dict_state(); },
                        EncodeOrDecodeState::Literal(_) => { self.cross_command_state.bk.obs_literal_state(); },
                        _ => {},
                    }
                    new_state = Some(command_state);
                },
                &mut EncodeOrDecodeState::BlockSwitchLiteral(ref mut block_type_state) => {
                    let src_block_switch_literal = match input_cmd {
                        &Command::BlockSwitchLiteral(bs) => bs,
                        _ => BlockSwitch::default(),
                    };
                    match block_type_state.encode_or_decode(&mut self.cross_command_state,
                                                            src_block_switch_literal,
                                                            BLOCK_TYPE_LITERAL_SWITCH,
                                                            input_bytes,
                                                            input_bytes_offset,
                                                            output_bytes,
                                                            output_bytes_offset) {
                        BrotliResult::ResultSuccess => {
                            self.cross_command_state.bk.obs_btypel(match block_type_state {
                                &mut BlockTypeState::FullyDecoded(btype) => btype,
                                _ => panic!("illegal output state"),
                            });
                            new_state = Some(EncodeOrDecodeState::Begin);
                        },
                        retval => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                },
                &mut EncodeOrDecodeState::BlockSwitchCommand(ref mut block_type_state) => {
                    let src_block_switch_command = match input_cmd {
                        &Command::BlockSwitchCommand(bs) => bs,
                        _ => BlockSwitch::default(),
                    };
                    match block_type_state.encode_or_decode(&mut self.cross_command_state,
                                                            src_block_switch_command,
                                                            BLOCK_TYPE_COMMAND_SWITCH,
                                                            input_bytes,
                                                            input_bytes_offset,
                                                            output_bytes,
                                                            output_bytes_offset) {
                        BrotliResult::ResultSuccess => {
                            self.cross_command_state.bk.obs_btypec(match block_type_state {
                                &mut BlockTypeState::FullyDecoded(btype) => btype,
                                _ => panic!("illegal output state"),
                            });
                            new_state = Some(EncodeOrDecodeState::Begin);
                        },
                        retval => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                },
                &mut EncodeOrDecodeState::BlockSwitchDistance(ref mut block_type_state) => {
                    let src_block_switch_distance = match input_cmd {
                        &Command::BlockSwitchDistance(bs) => bs,
                        _ => BlockSwitch::default(),
                    };

                    match block_type_state.encode_or_decode(&mut self.cross_command_state,
                                                            src_block_switch_distance,
                                                            BLOCK_TYPE_DISTANCE_SWITCH,
                                                            input_bytes,
                                                            input_bytes_offset,
                                                            output_bytes,
                                                            output_bytes_offset) {
                        BrotliResult::ResultSuccess => {
                            self.cross_command_state.bk.obs_btyped(match block_type_state {
                                &mut BlockTypeState::FullyDecoded(btype) => btype,
                                _ => panic!("illegal output state"),
                            });
                            new_state = Some(EncodeOrDecodeState::Begin);
                        },
                        retval => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                },
                &mut EncodeOrDecodeState::Copy(ref mut copy_state) => {
                    let backing_store = CopyCommand::nop();
                    let src_copy_command = self.cross_command_state.specialization.get_source_copy_command(input_cmd,
                                                                                                           &backing_store);
                    match copy_state.encode_or_decode(&mut self.cross_command_state,
                                                      src_copy_command,
                                                      input_bytes,
                                                      input_bytes_offset,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        BrotliResult::ResultSuccess => {
                            self.cross_command_state.bk.obs_distance(&copy_state.cc);
                            new_state = Some(EncodeOrDecodeState::PopulateRingBuffer(
                                Command::Copy(core::mem::replace(&mut copy_state.cc,
                                                                 CopyCommand::nop()))));
                        },
                        retval => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                },
                &mut EncodeOrDecodeState::Literal(ref mut lit_state) => {
                    let backing_store = LiteralCommand::nop();
                    let src_literal_command = self.cross_command_state.specialization.get_source_literal_command(input_cmd,
                                                                                                                 &backing_store);
                    match lit_state.encode_or_decode(&mut self.cross_command_state,
                                                      src_literal_command,
                                                      input_bytes,
                                                      input_bytes_offset,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        BrotliResult::ResultSuccess => {
                            new_state = Some(EncodeOrDecodeState::PopulateRingBuffer(
                                Command::Literal(core::mem::replace(&mut lit_state.lc,
                                                                    LiteralCommand::<AllocatedMemoryPrefix<AllocU8>>::nop()))));
                        },
                        retval => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                },
                &mut EncodeOrDecodeState::Dict(ref mut dict_state) => {
                    let backing_store = DictCommand::nop();
                    let src_dict_command = self.cross_command_state.specialization.get_source_dict_command(input_cmd,
                                                                                                                 &backing_store);
                    match dict_state.encode_or_decode(&mut self.cross_command_state,
                                                      src_dict_command,
                                                      input_bytes,
                                                      input_bytes_offset,
                                                      output_bytes,
                                                      output_bytes_offset
                                                      ) {
                        BrotliResult::ResultSuccess => {
                            new_state = Some(EncodeOrDecodeState::PopulateRingBuffer(
                                Command::Dict(core::mem::replace(&mut dict_state.dc,
                                                                 DictCommand::nop()))));
                        },
                        retval => {
                            return OneCommandReturn::BufferExhausted(retval);
                        }
                    }
                },
                &mut EncodeOrDecodeState::PopulateRingBuffer(ref mut o_cmd) => {
                    let mut tmp_output_offset_bytes_backing: usize = 0;
                    let mut tmp_output_offset_bytes = self.cross_command_state.specialization.get_recoder_output_offset(
                        output_bytes_offset,
                        &mut tmp_output_offset_bytes_backing);
                    match self.cross_command_state.recoder.encode_cmd(o_cmd,
                                                                  self.cross_command_state.
                                                                  specialization.get_recoder_output(output_bytes),
                                                                  tmp_output_offset_bytes) {
                        BrotliResult::NeedsMoreInput => panic!("Unexpected return value"),//new_state = Some(EncodeOrDecodeState::Begin),
                        BrotliResult::NeedsMoreOutput => {
                            self.cross_command_state.bk.decode_byte_count = self.cross_command_state.recoder.num_bytes_encoded() as u32;
                            if self.cross_command_state.specialization.does_caller_want_original_file_bytes() {
                                return OneCommandReturn::BufferExhausted(BrotliResult::NeedsMoreOutput); // we need the caller to drain the buffer
                            }
                            new_state = None;
                        },
                        BrotliResult::ResultFailure => {
                            self.cross_command_state.bk.decode_byte_count = self.cross_command_state.recoder.num_bytes_encoded() as u32;
                            return OneCommandReturn::BufferExhausted(Fail());
                        },
                        BrotliResult::ResultSuccess => {
                            self.cross_command_state.bk.command_count += 1;
                            self.cross_command_state.bk.decode_byte_count = self.cross_command_state.recoder.num_bytes_encoded() as u32;
                            // clobber bk.last_8_literals with the last 8 literals
                            let last_8 = self.cross_command_state.recoder.last_8_literals();
                            self.cross_command_state.bk.last_8_literals =
                                (last_8[0] as u64)
                                | ((last_8[1] as u64)<<0x8)
                                | ((last_8[2] as u64)<<0x10)
                                | ((last_8[3] as u64)<<0x18)
                                | ((last_8[4] as u64)<<0x20)
                                | ((last_8[5] as u64)<<0x28)
                                | ((last_8[6] as u64)<<0x30)
                                | ((last_8[7] as u64)<<0x38);
                            new_state = Some(EncodeOrDecodeState::Begin);
                            match o_cmd {
                                &mut Command::Literal(ref mut l) => {
                                    let mfd = core::mem::replace(&mut l.data,
                                                                 AllocatedMemoryPrefix::<AllocU8>::default()).0;
                                    self.cross_command_state.m8.free_cell(mfd);
                                },
                                _ => {},
                            }

                        },
                    }
                    // *o_cmd = core::mem::replace(&mut tmp_o_cmd[0], Command::nop())
                },
            }
            match new_state {
                Some(ns) => {
                    match ns {
                        EncodeOrDecodeState::Begin => {
                            self.state = EncodeOrDecodeState::Begin;
                            return OneCommandReturn::Advance;
                        },
                        _ => self.state = ns,
                    }
                },
                None => {},
            }
        }
    }
}
