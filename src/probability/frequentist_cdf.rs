use core;
use super::interface::{Prob, BaseCDF, Speed, CDF16, BLEND_FIXED_POINT_PRECISION, MAX_FREQUENTIST_PROB, DESERIALIZED_CDF_WEIGHT};
fn to_bit_i32(val: i32, shift_val: u8) -> u32 {
    if val != 0 {
        1 << shift_val
    } else {
        0
    }
}


#[derive(Clone,Copy)]
pub struct FrequentistCDF16 {
    pub cdf: [Prob; 16]
}

impl Default for FrequentistCDF16 {
    fn default() -> Self {
        FrequentistCDF16 {
            cdf: [4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60, 64]
        }
    }
}


impl BaseCDF for FrequentistCDF16 {
    fn num_symbols() -> u8 { 16 }
    fn used(&self) -> bool {
        self.entropy() != Self::default().entropy()
    }
    #[inline(always)]
    fn max(&self) -> Prob {
        self.cdf[15]
    }
    #[inline(always)]
    fn div_by_max(&self, val:i32) -> i32 {
        return val / i32::from(self.max())
    }
    fn log_max(&self) -> Option<i8> { None }
    #[inline(always)]
    fn cdf(&self, symbol: u8) -> Prob {
        self.cdf[symbol as usize & 0xf]
    }
    fn valid(&self) -> bool {
        let mut prev = 0;
        for item in self.cdf.split_at(15).0.iter() {
            if *item <= prev {
                return false;
            }
            prev = *item;
        }
        true
    }
}
const MIN_PDF_VAL: i16 = 1;
impl CDF16 for FrequentistCDF16 {
    #[inline(always)]
    fn average(&self, other:&Self, mix_rate:i32) -> Self {
        let mut retval = *self;
        let ourmax = i32::from(self.max());
        let othermax = i32::from(other.max());
        let ourmax_times_othermax = ourmax * othermax;
        let leading_zeros_combo = core::cmp::min(ourmax_times_othermax.leading_zeros(), 17);
        let desired_shift = 17 - leading_zeros_combo;
        let inv_mix_rate = (1 << BLEND_FIXED_POINT_PRECISION) - mix_rate;
        for (s, o) in retval.cdf.iter_mut().zip(other.cdf.iter()) {
          let rescaled_self = (i32::from(*s) * othermax) >> desired_shift;
          let rescaled_other = (i32::from(*o) * ourmax) >> desired_shift;
          *s = ((rescaled_self * mix_rate + rescaled_other * inv_mix_rate + 1) >> BLEND_FIXED_POINT_PRECISION) as Prob;
        }
        retval
    }
    #[inline(always)]
    fn blend(&mut self, symbol: u8, speed: Speed) {
        const CDF_BIAS : [Prob;16] = [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16];
        let increment : Prob = speed as Prob;
        for i in (symbol as usize)..16 {
            self.cdf[i] = self.cdf[i].wrapping_add(increment);
        }
        if self.cdf[15] >= MAX_FREQUENTIST_PROB {
            for i in 0..16 {
                self.cdf[i] = self.cdf[i].wrapping_add(CDF_BIAS[i]).wrapping_sub(self.cdf[i].wrapping_add(CDF_BIAS[i]) >> 2);
            }
        }
    }
    fn populate_from_pdf(&mut self, pop:&[u8]) {
       assert_eq!(pop.len(), 16);
       let loaded_cdf_weight = 2;
       self.cdf[0] = core::cmp::max(i16::from(pop[0]) << DESERIALIZED_CDF_WEIGHT, MIN_PDF_VAL);
       for index in 1..16 {
          self.cdf[index] = self.cdf[index - 1] + core::cmp::max((i16::from(pop[index]) << loaded_cdf_weight), MIN_PDF_VAL);
       }
    }
}

#[cfg(test)]
mod test {
    use super::FrequentistCDF16;
    declare_common_tests!(FrequentistCDF16);
}
