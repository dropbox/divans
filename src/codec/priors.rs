use ::interface::{
    CrossCommandBilling,
};
use ::priors::{PriorCollection, PriorMultiIndex};
pub const NUM_BLOCK_TYPES:usize = 256;
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use probability::BaseCDF;
define_prior_struct!(CrossCommandPriors, CrossCommandBilling,
                     (CrossCommandBilling::FullSelection, 4, NUM_BLOCK_TYPES),
                     (CrossCommandBilling::EndIndicator, 1, NUM_BLOCK_TYPES));



#[derive(PartialEq, Debug, Clone)]
pub enum LiteralNibblePriorType {
    FirstNibble,
    SecondNibble,
    CountSmall,
    SizeBegNib,
    SizeLastNib,
    SizeMantissaNib,
}

define_prior_struct!(LiteralCommandPriors, LiteralNibblePriorType,
                     (LiteralNibblePriorType::FirstNibble, 9, NUM_BLOCK_TYPES, 256),
                     (LiteralNibblePriorType::SecondNibble, 9, NUM_BLOCK_TYPES, 256),
                     (LiteralNibblePriorType::CountSmall, NUM_BLOCK_TYPES, 16),
                     (LiteralNibblePriorType::SizeBegNib, NUM_BLOCK_TYPES),
                     (LiteralNibblePriorType::SizeLastNib, NUM_BLOCK_TYPES),
                     (LiteralNibblePriorType::SizeMantissaNib, NUM_BLOCK_TYPES));
#[derive(PartialEq, Debug, Clone)]
pub enum CopyCommandNibblePriorType {
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
                     (CopyCommandNibblePriorType::DistanceBegNib, NUM_BLOCK_TYPES, NUM_COPY_COMMAND_ORGANIC_PRIORS),
                     (CopyCommandNibblePriorType::DistanceMnemonic, NUM_BLOCK_TYPES, 2),
                     (CopyCommandNibblePriorType::DistanceLastNib, NUM_BLOCK_TYPES, 1),
                     (CopyCommandNibblePriorType::DistanceMantissaNib, NUM_BLOCK_TYPES, 5),
                     (CopyCommandNibblePriorType::CountSmall, NUM_BLOCK_TYPES, NUM_COPY_COMMAND_ORGANIC_PRIORS),
                     (CopyCommandNibblePriorType::CountBegNib, NUM_BLOCK_TYPES, NUM_COPY_COMMAND_ORGANIC_PRIORS),
                     (CopyCommandNibblePriorType::CountLastNib, NUM_BLOCK_TYPES, NUM_COPY_COMMAND_ORGANIC_PRIORS),
                     (CopyCommandNibblePriorType::CountMantissaNib, NUM_BLOCK_TYPES, NUM_COPY_COMMAND_ORGANIC_PRIORS));
#[derive(PartialEq, Debug, Clone)]
pub enum DictCommandNibblePriorType {
    SizeBegNib,
    SizeLastNib,
    Index,
    Transform,
}

const NUM_ORGANIC_DICT_DISTANCE_PRIORS: usize = 5;
define_prior_struct!(DictCommandPriors, DictCommandNibblePriorType,
                     (DictCommandNibblePriorType::SizeBegNib, NUM_BLOCK_TYPES),
                     (DictCommandNibblePriorType::SizeLastNib, NUM_BLOCK_TYPES),
                     (DictCommandNibblePriorType::Index, NUM_BLOCK_TYPES, NUM_ORGANIC_DICT_DISTANCE_PRIORS),
                     (DictCommandNibblePriorType::Transform, 2, 25));

#[derive(PartialEq, Debug, Clone)]
pub enum BlockTypePriorType {
    Mnemonic,
    FirstNibble,
    SecondNibble,
    StrideNibble,
}
define_prior_struct!(BlockTypePriors, BlockTypePriorType,
                     (BlockTypePriorType::Mnemonic, 3), // 3 for each of ltype, ctype, dtype switches.
                     (BlockTypePriorType::FirstNibble, 3),
                     (BlockTypePriorType::SecondNibble, 3),
                     (BlockTypePriorType::StrideNibble, 1));

#[derive(PartialEq, Debug, Clone)]
pub enum PredictionModePriorType {
    Only,
    DynamicContextMixingSpeed,
    LiteralSpeed,
    Mnemonic,
    FirstNibble,
    SecondNibble,
}

define_prior_struct!(PredictionModePriors, PredictionModePriorType,
                     (PredictionModePriorType::Only, 1),
                     (PredictionModePriorType::LiteralSpeed, 1)
                     );
