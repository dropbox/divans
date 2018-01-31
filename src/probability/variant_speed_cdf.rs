use core;
use super::interface::{Prob, BaseCDF, Speed, CDF16, BLEND_FIXED_POINT_PRECISION, SPEED_PALETTE_SIZE, SymStartFreq};
use brotli::enc::util::FastLog2u16;

#[derive(Clone,Copy)]
pub struct VariantSpeedCDF<ChildCDF:BaseCDF+CDF16+Sized+Clone+Copy> {
    cdf: [ChildCDF; SPEED_PALETTE_SIZE + 1],
    cost: [f32;SPEED_PALETTE_SIZE+1],
}

impl<ChildCDF:BaseCDF+CDF16+Sized+Default> Default for VariantSpeedCDF<ChildCDF> {
    fn default() -> Self{
        VariantSpeedCDF {
            cdf:[ChildCDF::default();SPEED_PALETTE_SIZE + 1],
            cost:[0.0;SPEED_PALETTE_SIZE+1],
        }
    }
}

impl<ChildCDF:BaseCDF+CDF16+Sized+Default> CDF16 for VariantSpeedCDF<ChildCDF> {
    fn blend(&mut self, symbol: u8, dyn:Speed) {
        for (index, (cdf, cost)) in self.cdf.iter_mut().zip(self.cost.iter_mut()).enumerate() {
            let pdf = cdf.pdf(symbol);
            let max = cdf.max();
            *cost += FastLog2u16(max as u16) - FastLog2u16(pdf as u16);
            cdf.blend(symbol, if index == 0 {dyn} else {Speed::ENCODER_DEFAULT_PALETTE[index - 1]});
        }
    }
    fn average(&self, other: &Self, mix_rate: i32) ->Self {
        let mut ret = self.clone();
        ret.cdf[0] = self.cdf[0].average(&other.cdf[0], mix_rate);
        ret
    }
}

impl<ChildCDF:BaseCDF+CDF16+Sized> BaseCDF for VariantSpeedCDF<ChildCDF> {
    fn num_symbols() -> u8 {
        <ChildCDF as BaseCDF>::num_symbols()
    }
    fn cdf(&self, symbol: u8) -> Prob {
        self.cdf[0].cdf(symbol)
    }
    fn pdf(&self, symbol: u8) -> Prob {
        self.cdf[0].pdf(symbol)
    }
    fn div_by_max(&self, val: i32) -> i32 {
        self.cdf[0].div_by_max(val)
    }
    fn max(&self) -> Prob {
        self.cdf[0].max()
    }
    fn log_max(&self) -> Option<i8> {
        self.cdf[0].log_max()
    }
    fn used(&self) -> bool {
        self.cdf[0].used()
    }

    // returns true if valid.
    fn valid(&self) -> bool {
        self.cdf[0].valid()
    }

    // returns the entropy of the current distribution.
    fn entropy(&self) -> f64 {
        self.cdf[0].entropy()
    }
    #[inline(always)]
    fn sym_to_start_and_freq(&self,
                             sym: u8) -> SymStartFreq {
        self.cdf[0].sym_to_start_and_freq(sym)
    }
    #[inline(always)]
    fn rescaled_cdf(&self, sym: u8) -> i32 {
        self.cdf[0].rescaled_cdf(sym)
    }
    #[inline(always)]
    fn cdf_offset_to_sym_start_and_freq(&self,
                                        cdf_offset_p: Prob) -> SymStartFreq {
        self.cdf[0].cdf_offset_to_sym_start_and_freq(cdf_offset_p)
    }

    // These methods are optional because implementing them requires nontrivial bookkeeping.
    // Only CDFs that are intended for debugging should support them.
    fn num_samples(&self) -> Option<u32> {
        self.cdf[0].num_samples()
    }
    fn true_entropy(&self) -> Option<f64> {
        self.cdf[0].true_entropy()
    }
    fn rolling_entropy(&self) -> Option<f64> {
        self.cdf[0].rolling_entropy()
    }
    fn encoding_cost(&self) -> Option<f64> {
        self.cdf[0].encoding_cost()
    }
    fn num_variants(&self) -> usize {
        SPEED_PALETTE_SIZE
    }
    fn variant_cost(&self, variant_index: usize) -> f32 {
        self.cost[variant_index + 1]
    }
    fn base_variant_cost(&self) -> f32 {
        self.cost[0]
    }
}
