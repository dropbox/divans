use ::probability::{CDF2, CDF16};
use alloc::Allocator;
pub use super::interface::CrossCommandBookKeeping;

pub trait CodecTraits {
    const MIXING_PRIORS: bool;
    const HAVE_STRIDE: bool;
}
macro_rules! define_codec_trait {
    ($name: ident, $global: ident, mix: $mix: expr, have_stride: $have_stride: expr) => {
        #[derive(Default)]
        pub struct $name {}
        impl CodecTraits for $name {
            const MIXING_PRIORS: bool = $mix;
            const HAVE_STRIDE: bool = $have_stride;
        }
        pub static $global: $name = $name{};
    }
}
define_codec_trait!(MixingTrait, MIXING_TRAIT, mix: true, have_stride: true);
define_codec_trait!(DefaultTrait, DEFAULT_TRAIT, mix: false, have_stride: false);
define_codec_trait!(StrideTrait, STRIDE_TRAIT, mix: false, have_stride: true);

#[derive(Clone,Copy)]
pub enum CodecTraitSelector {
    DefaultTrait(&'static DefaultTrait),
    StrideTrait(&'static StrideTrait),
    MixingTrait(&'static MixingTrait),
}

pub fn construct_codec_trait_from_bookkeeping<Cdf16:CDF16,
                                           AllocU8:Allocator<u8>,
                                           AllocCDF2:Allocator<CDF2>,
                                           AllocCDF16:Allocator<Cdf16>>(
    bk:&CrossCommandBookKeeping<Cdf16,AllocU8, AllocCDF2, AllocCDF16>
) -> CodecTraitSelector {
    if bk.model_weights[0].should_mix() || bk.model_weights[1].should_mix() {
        return CodecTraitSelector::MixingTrait(&MIXING_TRAIT);
    }
    if bk.stride == 1 {
        return CodecTraitSelector::DefaultTrait(&DEFAULT_TRAIT);
    } else {
        return CodecTraitSelector::StrideTrait(&STRIDE_TRAIT);
    }
}
