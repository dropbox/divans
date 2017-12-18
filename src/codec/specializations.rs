use ::probability::{CDF2, CDF16};
use alloc::Allocator;
pub use super::interface::CrossCommandBookKeeping;

pub trait CodecTraits {
    fn materialized_prediction_mode(&self) -> bool;
    fn combine_literal_predictions(&self) -> bool;
    fn should_mix(&self, high_nibble:bool) -> bool;
}
macro_rules! define_codec_trait {
    ($name: ident, $global: ident, context_map: $cm:expr, combine: $combine: expr, mix: $mix: expr) => {
        #[derive(Default)]
        pub struct $name {}
        impl CodecTraits for $name {
            fn materialized_prediction_mode(&self) -> bool {
                $cm
            }
            fn combine_literal_predictions(&self) -> bool {
                $combine
            }
            fn should_mix(&self, _high_nibble:bool) -> bool {
                $mix
            }
        }
        pub static $global: $name = $name{};
    }
}
define_codec_trait!(MixingTrait, MIXING_TRAIT, context_map: true, combine: true, mix: true);

define_codec_trait!(AveragingTrait, AVERAGING_TRAIT, context_map: true, combine: true, mix: false);

define_codec_trait!(ContextMapTrait, CONTEXT_MAP_TRAIT, context_map: true, combine: false, mix: false);

define_codec_trait!(StrideTrait, STRIDE_TRAIT, context_map: false, combine: false, mix: false);



#[derive(Clone,Copy)]
pub enum CodecTraitSelector {
    AveragingTrait(&'static AveragingTrait),
    MixingTrait(&'static MixingTrait),
    ContextMapTrait(&'static ContextMapTrait),
    StrideTrait(&'static StrideTrait),
}

pub fn construct_codec_trait_from_bookkeeping<Cdf16:CDF16,
                                           AllocU8:Allocator<u8>,
                                           AllocCDF2:Allocator<CDF2>,
                                           AllocCDF16:Allocator<Cdf16>>(
    bk:&CrossCommandBookKeeping<Cdf16,AllocU8, AllocCDF2, AllocCDF16>
) -> CodecTraitSelector {
    if !bk.materialized_prediction_mode() {
        return CodecTraitSelector::StrideTrait(&STRIDE_TRAIT);
    }
    if !bk.combine_literal_predictions {
        return CodecTraitSelector::ContextMapTrait(&CONTEXT_MAP_TRAIT);
    }
    if bk.model_weights[0].should_mix() || bk.model_weights[1].should_mix() {
        return CodecTraitSelector::MixingTrait(&MIXING_TRAIT);
    }
    return CodecTraitSelector::AveragingTrait(&AVERAGING_TRAIT);
}
