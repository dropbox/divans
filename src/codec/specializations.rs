use ::probability::CDF16;
use alloc::Allocator;
pub use super::interface::{CrossCommandBookKeeping,LiteralBookKeeping};

pub trait CodecTraits {
    const MIXING_PRIORS: bool;
}
macro_rules! define_codec_trait {
    ($name: ident, $global: ident, mix: $mix: expr) => {
        #[derive(Default)]
        pub struct $name {}
        impl CodecTraits for $name {
            const MIXING_PRIORS: bool = $mix;
        }
        pub static $global: $name = $name{};
    }
}
define_codec_trait!(MixingTrait, MIXING_TRAIT, mix: true);
define_codec_trait!(DefaultTrait, DEFAULT_TRAIT, mix: false);

#[derive(Clone,Copy)]
pub enum CodecTraitSelector {
    DefaultTrait(&'static DefaultTrait),
    MixingTrait(&'static MixingTrait),
}

pub fn construct_codec_trait_from_bookkeeping<Cdf16:CDF16,
                                              AllocU8:Allocator<u8>,
                                              AllocCDF16:Allocator<Cdf16>>(
    lbk:&LiteralBookKeeping<Cdf16, AllocU8, AllocCDF16>,
) -> CodecTraitSelector {
    if lbk.model_weights[0].should_mix() || lbk.model_weights[1].should_mix() {
        return CodecTraitSelector::MixingTrait(&MIXING_TRAIT);
    }
    return CodecTraitSelector::DefaultTrait(&DEFAULT_TRAIT);
}

pub trait NibbleHalfTrait {
    const HIGH_NIBBLE: bool;
}

pub struct HighNibbleTrait {
}
impl NibbleHalfTrait for HighNibbleTrait {
    const HIGH_NIBBLE:bool = true;
}
pub static HIGH_NIBBLE_TRAIT: HighNibbleTrait = HighNibbleTrait{};

pub struct LowNibbleTrait {
}
impl NibbleHalfTrait for LowNibbleTrait {
    const HIGH_NIBBLE:bool = false;
}
pub static LOW_NIBBLE_TRAIT: LowNibbleTrait = LowNibbleTrait{};
