use ::probability::{CDF2, CDF16};
use alloc::Allocator;
pub use super::interface::CrossCommandBookKeeping;

pub trait CodecTraits {
    const MATERIALIZED_PREDICTION_MODE: bool;
    const COMBINE_LITERAL_PREDICTIONS: bool;
    const HAVE_STRIDE: bool;
}
macro_rules! define_codec_trait {
    ($name: ident, $global: ident, context_map: $cm:expr, combine: $combine: expr, mix: $mix: expr, have_stride: $have_stride: expr) => {
        #[derive(Default)]
        pub struct $name {}
        impl CodecTraits for $name {
            const MATERIALIZED_PREDICTION_MODE: bool = $cm;
            const COMBINE_LITERAL_PREDICTIONS: bool = $combine;
            const HAVE_STRIDE: bool = $have_stride;
        }
        pub static $global: $name = $name{};
    }
}
define_codec_trait!(MixingTrait, MIXING_TRAIT, context_map: true, combine: true, mix: true, have_stride: false);
define_codec_trait!(ContextMapTrait, CONTEXT_MAP_TRAIT, context_map: true, combine: false, mix: false, have_stride: false);
define_codec_trait!(StrideTrait, STRIDE_TRAIT, context_map: false, combine: false, mix: false, have_stride: false);

define_codec_trait!(StridedMixingTrait, MIXING_TRAIT_STRIDED, context_map: true, combine: true, mix: true, have_stride: true);
define_codec_trait!(StridedStrideTrait, STRIDE_TRAIT_STRIDED, context_map: false, combine: false, mix: false, have_stride: true);



#[derive(Clone,Copy)]
pub enum CodecTraitSelector {
    MixingTrait(&'static MixingTrait),
    ContextMapTrait(&'static ContextMapTrait),
    StrideTrait(&'static StrideTrait),
    StridedMixingTrait(&'static StridedMixingTrait),
    StridedStrideTrait(&'static StridedStrideTrait),
}

pub fn construct_codec_trait_from_bookkeeping<Cdf16:CDF16,
                                           AllocU8:Allocator<u8>,
                                           AllocCDF2:Allocator<CDF2>,
                                           AllocCDF16:Allocator<Cdf16>>(
    bk:&CrossCommandBookKeeping<Cdf16,AllocU8, AllocCDF2, AllocCDF16>
) -> CodecTraitSelector {
    if !bk.materialized_prediction_mode() {
        if bk.stride > 1 {
            return CodecTraitSelector::StridedStrideTrait(&STRIDE_TRAIT_STRIDED);
        } else {
            return CodecTraitSelector::StrideTrait(&STRIDE_TRAIT);
        }
    }
    if !bk.combine_literal_predictions {
        return CodecTraitSelector::ContextMapTrait(&CONTEXT_MAP_TRAIT);
    }
    if bk.stride > 1 {
        return CodecTraitSelector::StridedMixingTrait(&MIXING_TRAIT_STRIDED);
    } else {
        return CodecTraitSelector::MixingTrait(&MIXING_TRAIT);
    }
}
