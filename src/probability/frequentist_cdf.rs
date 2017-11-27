use core;
use super::interface::{Prob, BaseCDF, Speed, CDF16, BLEND_FIXED_POINT_PRECISION, SymStartFreq};
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
    fn max(&self) -> Prob {
        self.cdf[15]
    }
    fn div_by_max(&self, val:i32) -> i32 {
        return val / i32::from(self.max())
    }
    fn log_max(&self) -> Option<i8> { None }
    fn cdf(&self, symbol: u8) -> Prob {
        self.cdf[symbol as usize]
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


fn to_float(freq:Prob) -> f64 {
    return freq as f64 / (1i64 << 15) as f64
}

fn to_freq_array(other: FrequentistCDF16) -> [f64;16] {
    [to_float(other.sym_to_start_and_freq(0).range.freq),
     to_float(other.sym_to_start_and_freq(1).range.freq),
     to_float(other.sym_to_start_and_freq(2).range.freq),
     to_float(other.sym_to_start_and_freq(3).range.freq),
     to_float(other.sym_to_start_and_freq(4).range.freq),
     to_float(other.sym_to_start_and_freq(5).range.freq),
     to_float(other.sym_to_start_and_freq(6).range.freq),
     to_float(other.sym_to_start_and_freq(7).range.freq),
     to_float(other.sym_to_start_and_freq(8).range.freq),
     to_float(other.sym_to_start_and_freq(9).range.freq),
     to_float(other.sym_to_start_and_freq(10).range.freq),
     to_float(other.sym_to_start_and_freq(11).range.freq),
     to_float(other.sym_to_start_and_freq(12).range.freq),
     to_float(other.sym_to_start_and_freq(13).range.freq),
     to_float(other.sym_to_start_and_freq(14).range.freq),
     to_float(other.sym_to_start_and_freq(15).range.freq),
    ]
    /*
    [to_float(0, self.sym_to_start_and_freq(0).freq),
     to_float(self.sym_to_start_and_freq(0).freq, self.sym_to_start_and_freq(1)),
     to_float(self.sym_to_start_and_freq(1).freq, self.sym_to_start_and_freq(2)),
     to_float(self.sym_to_start_and_freq(2).freq, self.sym_to_start_and_freq(3)),
     to_float(self.sym_to_start_and_freq(3).freq, self.sym_to_start_and_freq(4)),
     to_float(self.sym_to_start_and_freq(4).freq, self.sym_to_start_and_freq(5)),
     to_float(self.sym_to_start_and_freq(5).freq, self.sym_to_start_and_freq(6)),
     to_float(self.sym_to_start_and_freq(6).freq, self.sym_to_start_and_freq(7)),
     to_float(self.sym_to_start_and_freq(7).freq, self.sym_to_start_and_freq(8)),
     to_float(self.sym_to_start_and_freq(8).freq, self.sym_to_start_and_freq(9)),
     to_float(self.sym_to_start_and_freq(9).freq, self.sym_to_start_and_freq(10)),
     to_float(self.sym_to_start_and_freq(10).freq, self.sym_to_start_and_freq(11)),
     to_float(self.sym_to_start_and_freq(11).freq, self.sym_to_start_and_freq(12)),
     to_float(self.sym_to_start_and_freq(12).freq, self.sym_to_start_and_freq(13)),
     to_float(self.sym_to_start_and_freq(13).freq, self.sym_to_start_and_freq(14)),
     to_float(self.sym_to_start_and_freq(14).freq, self.sym_to_start_and_freq(15))
    ] */   
}
fn stretch(x: f64) -> f64 {
    (x / (1.0 - x)).ln()
}
fn squash(x: f64) -> f64 {
    1.0f64/(1.0 + (-x).exp())
}
impl CDF16 for FrequentistCDF16 {
    fn average(&self, other:&Self, mix_rate:i32) -> Self {
        let mut retval = *self;
        let ourmax = i32::from(self.max());
        let othermax = i32::from(other.max());
        let ourmax_times_othermax = ourmax * othermax;
        let leading_zeros_combo = core::cmp::min(ourmax_times_othermax.leading_zeros(), 17);
        let desired_shift = 17 - leading_zeros_combo;
        let inv_mix_rate = (1 << BLEND_FIXED_POINT_PRECISION) - mix_rate;
        let float_mix_rate = mix_rate as f64 / (1 << BLEND_FIXED_POINT_PRECISION) as f64;
        let float_inv_mix_rate = 1.0 - float_mix_rate;
        let self_rescaled_cdf = to_freq_array(*self);
        let other_rescaled_cdf = to_freq_array(*other);
        let mut joined_cdf = [0.0f64; 16];
        for i in 0..16 {
            joined_cdf[i] = squash(stretch(self_rescaled_cdf[i]) * float_mix_rate + stretch(other_rescaled_cdf[i]) * float_inv_mix_rate)
        }
        let mut cumul:Prob = 0;
        for (s, o) in retval.cdf.iter_mut().zip(joined_cdf.iter()) {
            cumul += ((*o) * (1i64 << 15) as f64) as Prob;
            *s = cumul;
        }
        retval
    }
    fn blend(&mut self, symbol: u8, speed: Speed) {
        const CDF_BIAS : [Prob;16] = [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16];
        let increment : Prob = speed as Prob;
        for i in (symbol as usize)..16 {
            self.cdf[i] = self.cdf[i].wrapping_add(increment);
        }
        let limit: Prob = 32_767 - 16 - 384 /* XXX: max possible increment */;
        if self.cdf[15] >= limit {
            for i in 0..16 {
                self.cdf[i] = self.cdf[i].wrapping_add(CDF_BIAS[i]).wrapping_sub(self.cdf[i].wrapping_add(CDF_BIAS[i]) >> 2);
            }
        }
    }
}
