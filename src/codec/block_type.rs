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
    FullyDecoded(u8),
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
            if input_bs.block_type() == superstate.bk.btype_lru[block_type_switch_index][1] {
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
                        0 => *self = BlockTypeState::FullyDecoded(
                            superstate.bk.btype_lru[block_type_switch_index][1]),
                        1 => *self = BlockTypeState::FullyDecoded(
                            superstate.bk.btype_max_seen[block_type_switch_index].wrapping_add(1)),
                        15 => *self = BlockTypeState::TwoNibbleType,
                        val => *self = BlockTypeState::FullyDecoded(val - 2),
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
                    *self = BlockTypeState::FullyDecoded((second_nibble << 4) | first_nibble);
                }
                BlockTypeState::FullyDecoded(_) =>   {
                    return BrotliResult::ResultSuccess;
                }
            }
        }
    }
}

#[derive(Clone,Copy)]
pub enum LiteralBlockTypeState {
    Begin,
    Intermediate(BlockTypeState),
    StrideNibble(u8),
    FullyDecoded(u8, u8),
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
