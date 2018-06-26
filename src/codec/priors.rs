use ::interface::{
    CrossCommandBilling,
};
use ::priors::{PriorCollection, PriorMultiIndex};
#[cfg(feature="billing")]
#[cfg(feature="debug_entropy")]
use ::priors::summarize_prior_billing;
pub const NUM_BLOCK_TYPES:usize = 256;
pub const NUM_STRIDES:usize = 8;
use alloc::{SliceWrapper, Allocator, SliceWrapperMut};
use probability::{BaseCDF};
define_prior_struct!(CrossCommandPriors, CrossCommandBilling,
                     (CrossCommandBilling::FullSelection, 16, 1),
                     (CrossCommandBilling::EndIndicator, 1, NUM_BLOCK_TYPES));



#[derive(PartialEq, Debug, Clone)]
pub enum LiteralCommandPriorType {
    CountSmall,
    SizeBegNib,
    SizeLastNib,
    SizeMantissaNib,
}
#[derive(PartialEq, Debug, Clone)]
pub enum LiteralCMPriorType {
    FirstNibble,
    SecondNibble,
}
#[derive(PartialEq, Debug, Clone)]
pub enum LiteralNibblePriorType {
    CombinedNibble,
}

define_prior_struct!(LiteralNibblePriors, LiteralNibblePriorType,
                     (LiteralNibblePriorType::CombinedNibble, 6, 256, NUM_BLOCK_TYPES)
                     );
                     
define_prior_struct!(LiteralCommandPriors, LiteralCommandPriorType,
                     (LiteralCommandPriorType::CountSmall, NUM_BLOCK_TYPES, 16),
                     (LiteralCommandPriorType::SizeBegNib, NUM_BLOCK_TYPES),
                     (LiteralCommandPriorType::SizeLastNib, NUM_BLOCK_TYPES),
                     (LiteralCommandPriorType::SizeMantissaNib, NUM_BLOCK_TYPES));

define_prior_struct!(LiteralCommandPriorsCM, LiteralCMPriorType,
                     (LiteralCMPriorType::FirstNibble, 1, NUM_BLOCK_TYPES),
                     (LiteralCMPriorType::SecondNibble, 1, 16, NUM_BLOCK_TYPES));

#[derive(PartialEq, Debug, Clone)]
pub enum RandLiteralNibblePriorType {
    CountSmall,
    SizeBegNib,
    SizeLastNib,
    SizeMantissaNib,
}
define_prior_struct!(RandLiteralCommandPriors, RandLiteralNibblePriorType,
                     (RandLiteralNibblePriorType::CountSmall, NUM_BLOCK_TYPES, 16),
                     (RandLiteralNibblePriorType::SizeBegNib, NUM_BLOCK_TYPES),
                     (RandLiteralNibblePriorType::SizeLastNib, NUM_BLOCK_TYPES),
                     (RandLiteralNibblePriorType::SizeMantissaNib, NUM_BLOCK_TYPES));

#[derive(PartialEq, Debug, Clone)]
pub enum CopyCommandNibblePriorType {
    DistanceBegNib,
    DistanceLastNib,
    DistanceMnemonic,
    DistanceMnemonicTwo,
    DistanceMantissaNib,
    DistanceAlignNib,
    DistanceDirectNib,
    CountSmall,
    CountBegNib,
    CountLastNib,
    CountMantissaNib,
}
const NUM_COPY_COMMAND_ORGANIC_PRIORS: usize = 64;
define_prior_struct!(CopyCommandPriors, CopyCommandNibblePriorType,
                     (CopyCommandNibblePriorType::DistanceBegNib, NUM_BLOCK_TYPES, NUM_COPY_COMMAND_ORGANIC_PRIORS),
                     (CopyCommandNibblePriorType::DistanceMnemonic, NUM_BLOCK_TYPES, 4),
                     (CopyCommandNibblePriorType::DistanceLastNib, NUM_BLOCK_TYPES, 8),
                     (CopyCommandNibblePriorType::DistanceAlignNib, 1),
                     (CopyCommandNibblePriorType::DistanceDirectNib, 64, 64),
                     (CopyCommandNibblePriorType::DistanceMantissaNib, 512, 2),
                     (CopyCommandNibblePriorType::CountSmall, 2, NUM_BLOCK_TYPES, NUM_COPY_COMMAND_ORGANIC_PRIORS),
                     (CopyCommandNibblePriorType::CountBegNib, 2, NUM_BLOCK_TYPES, NUM_COPY_COMMAND_ORGANIC_PRIORS),
                     (CopyCommandNibblePriorType::CountLastNib, 2, NUM_BLOCK_TYPES, NUM_COPY_COMMAND_ORGANIC_PRIORS),
                     (CopyCommandNibblePriorType::CountMantissaNib, 2, NUM_BLOCK_TYPES, 2*NUM_COPY_COMMAND_ORGANIC_PRIORS));
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
    PriorDepth,
    PriorAlgorithm,
    PriorMixingValue,
    LiteralSpeed,
    Mnemonic,
    FirstNibble,
    SecondNibble,
    ContextMapSpeedPalette,
}

define_prior_struct!(PredictionModePriors, PredictionModePriorType,
                     (PredictionModePriorType::PriorAlgorithm, 4),
                     (PredictionModePriorType::Only, 1),
                     (PredictionModePriorType::LiteralSpeed, 1),
                     (PredictionModePriorType::FirstNibble, 2),
                     (PredictionModePriorType::SecondNibble, 2),
                     (PredictionModePriorType::Mnemonic, 4),
                     (PredictionModePriorType::PriorMixingValue, 17),
                     (PredictionModePriorType::ContextMapSpeedPalette, 4)
                     );

pub struct PriorAlgorithm(u16);
impl PriorAlgorithm {
    pub fn serialize(&self) -> u16 {
        self.0
    }
    pub fn deserialize(data:u16) -> PriorAlgorithm {
        PriorAlgorithm(data)
    }
    pub fn non_default(&self) -> bool {
       self.0 != 0
    }
    pub fn use_lzma_command_type(&self) -> bool {
        (self.0 & 1) != 0
    }
    pub fn set_lzma_command_type(&mut self) {
        self.0 |= 1;
    }

    pub fn use_lzma_distance_order(&self) -> bool {
        (self.0 & 2) != 0
    }
    pub fn set_lzma_distance_order(&mut self) {
        self.0 |= 2;
    }

    pub fn use_lzma_copy_prior(&self) -> bool {
        (self.0 & 4) != 0
    }
    pub fn set_lzma_copy_prior(&mut self) {
        self.0 |= 4;
    }
    pub fn use_lzma_distance_prior(&self) -> bool {
        (self.0 & 8) != 0
    }
    pub fn set_lzma_distance_prior(&mut self) {
        self.0 |= 8;
    }
}

impl Default for PriorAlgorithm {
    fn default() -> Self {
        PriorAlgorithm(0)
    }
}
