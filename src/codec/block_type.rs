use brotli::BrotliResult;
use alloc::Allocator;
use super::interface::{
    EncoderOrDecoderSpecialization,
    CrossCommandState,
    BLOCK_TYPE_LITERAL_SWITCH,
};
use ::interface::{
    ArithmeticEncoderOrDecoder,
    BillingDesignation,
    CrossCommandBilling,
    BlockSwitch,
    LiteralBlockSwitch,
};
use ::probability::{Speed, CDF2, CDF16};
use ::priors::PriorCollection;
use super::priors::{BlockTypePriorType};
#[derive(Clone,Copy,PartialEq,Eq, Hash, Debug)]
pub enum BlockTypeState {
    Begin,
    TwoNibbleType,
    FinalNibble(u8),
    CountExp(u8),
    CountMantissa(u8, u8, u8, u32),
    FullyDecoded(BlockSwitch),
}


impl BlockTypeState {
    pub fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
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
            if input_bs.block_type() == superstate.bk.btype_lru[block_type_switch_index][1].block_type() {
                0
            } else if input_bs.block_type() == superstate.bk.btype_max_seen[block_type_switch_index].wrapping_add(1) {
                1
            } else if input_bs.block_type() <= 12 {
                input_bs.block_type() + 2
            } else {
                15
            };
        let mut first_nibble:u8 = input_bs.block_type() & 0xf;
        let mut second_nibble:u8 = input_bs.block_type() >> 4;
        let (mut count_exp_code, count_mantissa) = distance_code_from_input(input_bs.count());
        loop {
            match superstate.coder.drain_or_fill_internal_buffer(input_bytes,
                                                                 input_offset,
                                                                 output_bytes,
                                                                 output_offset) {
                BrotliResult::ResultSuccess => {},
                need_something => return need_something,
            }
            let billing = BillingDesignation::CrossCommand(CrossCommandBilling::BlockSwitchType);
            match *self {
                BlockTypeState::Begin => {
                    let mut nibble_prob = superstate.bk.btype_priors.get(BlockTypePriorType::Mnemonic,
                                                                         (block_type_switch_index,));
                    superstate.coder.get_or_put_nibble(&mut varint_nibble, nibble_prob, billing);
                    nibble_prob.blend(varint_nibble, Speed::SLOW);
                    match varint_nibble {
                        0 => *self = BlockTypeState::CountExp(
                            superstate.bk.btype_lru[block_type_switch_index][1].block_type()),
                        1 => *self = BlockTypeState::CountExp(
                            superstate.bk.btype_max_seen[block_type_switch_index].wrapping_add(1)),
                        15 => *self = BlockTypeState::TwoNibbleType,
                        val => *self = BlockTypeState::CountExp(val - 2),
                    }
                },
                BlockTypeState::TwoNibbleType => {
                    let mut nibble_prob = superstate.bk.btype_priors.get(BlockTypePriorType::FirstNibble,
                                                                         (block_type_switch_index,));
                    superstate.coder.get_or_put_nibble(&mut first_nibble, nibble_prob, billing);
                    nibble_prob.blend(first_nibble, Speed::SLOW);
                    *self = BlockTypeState::FinalNibble(first_nibble);
                },
                BlockTypeState::FinalNibble(first_nibble) => {
                    let mut nibble_prob = superstate.bk.btype_priors.get(BlockTypePriorType::SecondNibble,
                                                                         (block_type_switch_index,));
                    superstate.coder.get_or_put_nibble(&mut second_nibble, nibble_prob, billing);
                    nibble_prob.blend(second_nibble, Speed::SLOW);
                    *self = BlockTypeState::CountExp((second_nibble << 4) | first_nibble);
                }
                BlockTypeState::CountExp(typ) => {
                    let mut nibble_prob = superstate.bk.btype_priors.get(BlockTypePriorType::CountExp,
                                                                         (block_type_switch_index,));
                    superstate.coder.get_or_put_nibble(&mut count_exp_code, nibble_prob, billing);
                    nibble_prob.blend(count_exp_code, Speed::SLOW);
                    *self = BlockTypeState::CountMantissa(typ, count_exp_code, distance_code_num_to_decode(count_exp_code), 0);
                }
                BlockTypeState::CountMantissa(typ, count_exp, num_to_decode, total) => {
                    let mut nibble_prob = superstate.bk.btype_priors.get(BlockTypePriorType::CountMantissa,
                                                                         (block_type_switch_index, num_to_decode as usize * (distance_code_num_to_decode(count_exp) == num_to_decode) as usize));
                    let shift = (num_to_decode - 1)<<2;
                    let mut count_mantissa_nibble = (count_mantissa >> shift) as u8 & 0xf;
                    superstate.coder.get_or_put_nibble(&mut count_mantissa_nibble, nibble_prob, billing);
                    nibble_prob.blend(count_mantissa_nibble, Speed::SLOW);
                    let new_mantissa = total | ((count_mantissa_nibble as u32) << shift);
                    if num_to_decode > 1 {
                        *self = BlockTypeState::CountMantissa(typ, count_exp, num_to_decode - 1, new_mantissa);
                    } else {
                        *self = BlockTypeState::FullyDecoded(BlockSwitch::new(typ, reassemble_distance_code_from(count_exp, new_mantissa)));
                    }
                }
                BlockTypeState::FullyDecoded(_) =>   {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
}

pub fn distance_code_from_input(val: u32) -> (u8, u32) {
    if val < 16 {
        return (0, val);
    }
    if val < 16 + 256 {
        return (1, val - 16);
    }
    if val < 16 + 256 + 4096{
        return (2, val - 16 - 256);
    }
    if val < 16 + 256 + 4096 + 65536{
        return (3, val - 16 - 256 - 4096);
    }
    if val < 16 + 256 + 4096 + 65536 + 65536 * 16{
        return (4, val - 16 - 256 - 4096 - 65536);
    }
    let max = 16 + 256 + 4096 + 65536 + 65536 * 16 + 65536 * 256 - 1;
    if val <= max {
        return (5, val - 16 - 256 - 4096 - 65536 - 65536 * 16);
    }
    (5, max)
}
pub fn distance_code_num_to_decode(val: u8) -> u8 {
    match val {
        0 => 1,
        1 => 2,
        2 => 3,
        3 => 4,
        4 => 5,
        5 => 6,
        _ => unreachable!(),
    }
}

pub fn reassemble_distance_code_from(exp_code: u8, mantissa: u32) -> u32 {
    match exp_code {
        0 => mantissa,
        1 => mantissa + 16,
        2 => mantissa + 16 + 256,
        3 => mantissa + 16 + 256 + 4096,
        4 => mantissa + 16 + 256 + 4096 + 65536,
        5 => mantissa + 16 + 256 + 4096 + 65536 + 16 * 65536,
        _ => unreachable!(),
    }
}
mod test {
    #[test]
    fn test_distance_code_from_input() {
        let test_set = [0u32, 1u32, 2u32, 16u32, 63u32, 255u32,
                        256u32,256+15u32,256+16u32,
                        4096u32, 4096u32 + 256 + 15,4096u32 + 256 + 16,
                        4096u32 + 256 + 17,4096u32 + 256 + 15 + 65536,
                        4096u32 + 256 + 16 + 65536,4096u32 + 256 + 17 + 65536,
                        4096u32 + 256 + 15 + 65536 + 16*65536,
                        4096u32 + 256 + 16 + 65536 + 16*65536,
                        4096u32 + 256 + 17 + 65536 + 16*65536,
                        (1u32<<24) - 1,
                        1u32<<24];
        for item in test_set.iter() {
            let (ec, m) = super::distance_code_from_input(*item);
            let size = super::distance_code_num_to_decode(ec);
            assert!(m < (1<<(size <<2)));
            let cand = super::reassemble_distance_code_from(ec, m);
            assert_eq!(cand, *item);
        }
                        
    }
    
}

#[derive(Clone,Copy)]
pub enum LiteralBlockTypeState {
    Begin,
    Intermediate(BlockTypeState),
    StrideNibble(BlockSwitch),
    FullyDecoded(BlockSwitch, u8),
}

impl LiteralBlockTypeState {
    pub fn encode_or_decode<ArithmeticCoder:ArithmeticEncoderOrDecoder,
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
        input_bs: LiteralBlockSwitch,
        input_bytes: &[u8],
        input_offset: &mut usize,
        output_bytes: &mut [u8],
        output_offset: &mut usize) -> BrotliResult {
        loop {
            let billing = BillingDesignation::CrossCommand(CrossCommandBilling::BlockSwitchType);
            match *self {
                LiteralBlockTypeState::Begin => {
                    *self = LiteralBlockTypeState::Intermediate(BlockTypeState::Begin);
                },
                LiteralBlockTypeState::Intermediate(bts) => {
                    let mut local_bts = bts;
                    let early_ret = match local_bts.encode_or_decode(superstate,
                      input_bs.0,
                      BLOCK_TYPE_LITERAL_SWITCH,
                      input_bytes,
                      input_offset,
                      output_bytes,
                      output_offset) {
                        BrotliResult::ResultSuccess => None,
                        any => Some(any),
                    };
                    match local_bts {
                        BlockTypeState::FullyDecoded(val) => {
                           *self = LiteralBlockTypeState::StrideNibble(val);
                        }
                        any => {
                           *self = LiteralBlockTypeState::Intermediate(any);
                        }
                    }
                    if let Some(val) = early_ret {
                       return val;
                    }
                },
                LiteralBlockTypeState::StrideNibble(ltype) =>   {
                     match superstate.coder.drain_or_fill_internal_buffer(input_bytes,
                                                                 input_offset,
                                                                 output_bytes,
                                                                 output_offset) {
                         BrotliResult::ResultSuccess => {},
                         need_something => return need_something,
                    }
                        let mut stride_nibble = match superstate.bk.desired_force_stride {
                        super::StrideSelection::UseBrotliRec => input_bs.stride(),
                        matched_stride => matched_stride as u8,
                    };
                    let mut nibble_prob = superstate.bk.btype_priors.get(BlockTypePriorType::StrideNibble,
                                                                         (0,));
                    superstate.coder.get_or_put_nibble(&mut stride_nibble, nibble_prob, billing);
                    nibble_prob.blend(stride_nibble, Speed::SLOW);
                    *self = LiteralBlockTypeState::FullyDecoded(ltype, stride_nibble);
                },
                LiteralBlockTypeState::FullyDecoded(_ltype, _stride) => {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
}
