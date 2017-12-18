use ::probability::{CDF2, CDF16};
use alloc::Allocator;
use ::interface::{
    LiteralPredictionModeNibble,
    LITERAL_PREDICTION_MODE_SIGN,
    LITERAL_PREDICTION_MODE_UTF8,
    LITERAL_PREDICTION_MODE_MSB6,
    LITERAL_PREDICTION_MODE_LSB6,
};
pub use super::interface::CrossCommandBookKeeping;

pub trait CodecTraits {
    fn materialized_prediction_mode(&self) -> bool;
    fn combine_literal_predictions(&self) -> bool;
    fn should_mix(&self, high_nibble:bool) -> bool;
    fn literal_prediction_mode(&self) -> u8;
}
macro_rules! define_codec_trait {
    ($name: ident, $global: ident, context_map: $cm:expr, combine: $combine: expr, mix: $mix: expr, pred: $prediction_mode: expr) => {
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
            fn literal_prediction_mode(&self) -> u8 {
                $prediction_mode
            }
        }
        pub static $global: $name = $name{};
    }
}
define_codec_trait!(MixingTraitSign, MIXING_TRAIT_SIGN, context_map: true, combine: true, mix: true, pred: LITERAL_PREDICTION_MODE_SIGN);
define_codec_trait!(MixingTraitUTF8, MIXING_TRAIT_UTF8, context_map: true, combine: true, mix: true, pred: LITERAL_PREDICTION_MODE_UTF8);
define_codec_trait!(MixingTraitMSB6, MIXING_TRAIT_MSB6, context_map: true, combine: true, mix: true, pred: LITERAL_PREDICTION_MODE_MSB6);
define_codec_trait!(MixingTraitLSB6, MIXING_TRAIT_LSB6, context_map: true, combine: true, mix: true, pred: LITERAL_PREDICTION_MODE_LSB6);

define_codec_trait!(AveragingTraitSign, AVERAGING_TRAIT_SIGN, context_map: true, combine: true, mix: false, pred: LITERAL_PREDICTION_MODE_SIGN);
define_codec_trait!(AveragingTraitUTF8, AVERAGING_TRAIT_UTF8, context_map: true, combine: true, mix: false, pred: LITERAL_PREDICTION_MODE_UTF8);
define_codec_trait!(AveragingTraitMSB6, AVERAGING_TRAIT_MSB6, context_map: true, combine: true, mix: false, pred: LITERAL_PREDICTION_MODE_MSB6);
define_codec_trait!(AveragingTraitLSB6, AVERAGING_TRAIT_LSB6, context_map: true, combine: true, mix: false, pred: LITERAL_PREDICTION_MODE_LSB6);


define_codec_trait!(ContextMapTraitSign, CONTEXT_MAP_TRAIT_SIGN, context_map: true, combine: false, mix: false, pred: LITERAL_PREDICTION_MODE_SIGN);
define_codec_trait!(ContextMapTraitUTF8, CONTEXT_MAP_TRAIT_UTF8, context_map: true, combine: false, mix: false, pred: LITERAL_PREDICTION_MODE_UTF8);
define_codec_trait!(ContextMapTraitMSB6, CONTEXT_MAP_TRAIT_MSB6, context_map: true, combine: false, mix: false, pred: LITERAL_PREDICTION_MODE_MSB6);
define_codec_trait!(ContextMapTraitLSB6, CONTEXT_MAP_TRAIT_LSB6, context_map: true, combine: false, mix: false, pred: LITERAL_PREDICTION_MODE_LSB6);

define_codec_trait!(StrideTraitSign, STRIDE_TRAIT_SIGN, context_map: false, combine: false, mix: false, pred: LITERAL_PREDICTION_MODE_SIGN);
define_codec_trait!(StrideTraitUTF8, STRIDE_TRAIT_UTF8, context_map: false, combine: false, mix: false, pred: LITERAL_PREDICTION_MODE_UTF8);
define_codec_trait!(StrideTraitMSB6, STRIDE_TRAIT_MSB6, context_map: false, combine: false, mix: false, pred: LITERAL_PREDICTION_MODE_MSB6);
define_codec_trait!(StrideTraitLSB6, STRIDE_TRAIT_LSB6, context_map: false, combine: false, mix: false, pred: LITERAL_PREDICTION_MODE_LSB6);



#[derive(Clone,Copy)]
pub enum CodecTraitSelector {
    AveragingTraitSign(&'static AveragingTraitSign),
    MixingTraitSign(&'static MixingTraitSign),
    ContextMapTraitSign(&'static ContextMapTraitSign),
    StrideTraitSign(&'static StrideTraitSign),

    AveragingTraitUTF8(&'static AveragingTraitUTF8),
    MixingTraitUTF8(&'static MixingTraitUTF8),
    ContextMapTraitUTF8(&'static ContextMapTraitUTF8),
    StrideTraitUTF8(&'static StrideTraitUTF8),

    AveragingTraitMSB6(&'static AveragingTraitMSB6),
    MixingTraitMSB6(&'static MixingTraitMSB6),
    ContextMapTraitMSB6(&'static ContextMapTraitMSB6),
    StrideTraitMSB6(&'static StrideTraitMSB6),

    AveragingTraitLSB6(&'static AveragingTraitLSB6),
    MixingTraitLSB6(&'static MixingTraitLSB6),
    ContextMapTraitLSB6(&'static ContextMapTraitLSB6),
    StrideTraitLSB6(&'static StrideTraitLSB6),
}

pub fn construct_codec_trait_from_bookkeeping<Cdf16:CDF16,
                                           AllocU8:Allocator<u8>,
                                           AllocCDF2:Allocator<CDF2>,
                                           AllocCDF16:Allocator<Cdf16>>(
    bk:&CrossCommandBookKeeping<Cdf16,AllocU8, AllocCDF2, AllocCDF16>
) -> CodecTraitSelector {
    if !bk.materialized_prediction_mode() {
        return match bk.literal_prediction_mode {
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_SIGN) =>
                CodecTraitSelector::StrideTraitSign(&STRIDE_TRAIT_SIGN),
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_UTF8) =>
                CodecTraitSelector::StrideTraitUTF8(&STRIDE_TRAIT_UTF8),
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_MSB6) =>
                CodecTraitSelector::StrideTraitMSB6(&STRIDE_TRAIT_MSB6),
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_LSB6) =>
                CodecTraitSelector::StrideTraitLSB6(&STRIDE_TRAIT_LSB6),
            _ => panic!("Internal Error: parsed nibble prediction mode has more than 2 bits"),
        };
    }
    if !bk.combine_literal_predictions {
        return match bk.literal_prediction_mode {
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_SIGN) =>
                CodecTraitSelector::ContextMapTraitSign(&CONTEXT_MAP_TRAIT_SIGN),
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_UTF8) =>
                CodecTraitSelector::ContextMapTraitUTF8(&CONTEXT_MAP_TRAIT_UTF8),
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_MSB6) =>
                CodecTraitSelector::ContextMapTraitMSB6(&CONTEXT_MAP_TRAIT_MSB6),
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_LSB6) =>
                CodecTraitSelector::ContextMapTraitLSB6(&CONTEXT_MAP_TRAIT_LSB6),
            _ => panic!("Internal Error: parsed nibble prediction mode has more than 2 bits"),
        };
    }
    if bk.model_weights[0].should_mix() || bk.model_weights[1].should_mix() {
        return match bk.literal_prediction_mode {
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_SIGN) =>
                CodecTraitSelector::MixingTraitSign(&MIXING_TRAIT_SIGN),
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_UTF8) =>
                CodecTraitSelector::MixingTraitUTF8(&MIXING_TRAIT_UTF8),
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_MSB6) =>
                CodecTraitSelector::MixingTraitMSB6(&MIXING_TRAIT_MSB6),
            LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_LSB6) =>
                CodecTraitSelector::MixingTraitLSB6(&MIXING_TRAIT_LSB6),
            _ => panic!("Internal Error: parsed nibble prediction mode has more than 2 bits"),
        };
    }
    return match bk.literal_prediction_mode {
        LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_SIGN) =>
            CodecTraitSelector::AveragingTraitSign(&AVERAGING_TRAIT_SIGN),
        LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_UTF8) =>
            CodecTraitSelector::AveragingTraitUTF8(&AVERAGING_TRAIT_UTF8),
        LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_MSB6) =>
            CodecTraitSelector::AveragingTraitMSB6(&AVERAGING_TRAIT_MSB6),
        LiteralPredictionModeNibble(LITERAL_PREDICTION_MODE_LSB6) =>
            CodecTraitSelector::AveragingTraitLSB6(&AVERAGING_TRAIT_LSB6),
        _ => panic!("Internal Error: parsed nibble prediction mode has more than 2 bits"),
    };
}
