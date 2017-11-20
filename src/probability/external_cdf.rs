use core;
use super::interface::{BaseCDF, Prob, CDF16, Speed, BLEND_FIXED_POINT_PRECISION};

#[derive(Clone,Copy)]
pub struct ExternalProbCDF16 {
    pub cdf: [Prob; 16],
    pub nibble: usize,
}

impl Default for ExternalProbCDF16 {
    fn default() -> Self {
        ExternalProbCDF16 {
            cdf: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            nibble: 0,
        }
    }
}

impl ExternalProbCDF16 {
    pub fn init<T: BaseCDF>(&mut self, _n: u8, probs: &[u8], mix: &T) {
        //println_stderr!("init for {:x}", _n);
        //println_stderr!("init for {:x} {:x} {:x} {:x}", probs[0], probs[1], probs[2], probs[3]);
        //average the two probabilities
        assert!(probs.len() == 4);
        self.nibble = _n as usize;
        let mut pcdf = [1f64;16];
        for nibble in 0..16 {
            //println_stderr!("setting for {:x}", nibble);
            for bit in 0..4 {
                let p1 = f64::from(probs[bit]) / f64::from(u8::max_value());
                let isone = (nibble & (1<<(3 - bit))) != 0;
                //println_stderr!("bit {:} is {:} {:}", bit, isone, p1);
                if isone {
                    pcdf[nibble] *= p1;
                } else {
                    pcdf[nibble] *= 1f64 - p1;
                }
            }
        }
        let mut mcdf = [1f64;16];
        for nibble in 1..16 {
            let prev = nibble - 1;
            let c = f64::from(mix.cdf(nibble));
            let p = f64::from(mix.cdf(prev));
            let m = f64::from(mix.max());
            let d = (c - p) / m;
            assert!(d < 1.0);
            mcdf[nibble as usize] = d;
        }
        for nibble in 0..16 {
            pcdf[nibble] = (pcdf[nibble] + mcdf[nibble])/2f64;
        }
        let mut sum = 0f64;
        for pcdf_nibble in &mut pcdf {
            sum += *pcdf_nibble;
            *pcdf_nibble = sum;
        }
        for pcdf_nibble in &mut pcdf {
            *pcdf_nibble /= sum;
        }
        for nibble in 0..16 {
            let p = pcdf[nibble];
            let res = (p * f64::from(Prob::max_value())) as Prob;
            let least1 = core::cmp::max(res, 1);
            self.cdf[nibble] = core::cmp::min(least1, self.max() - 1);
            //println_stderr!("cdf set {:x} {:x} {:}", nibble, self.cdf[nibble], p);
        }
    }
}

impl BaseCDF for ExternalProbCDF16 {
    fn num_symbols() -> u8 { 16 }
    fn div_by_max(&self, val:i32) -> i32 {
        return val / i32::from(self.max())
    }
    fn used(&self) -> bool {
        self.entropy() != Self::default().entropy()
    }
    fn max(&self) -> Prob {
        Prob::max_value()
    }
    fn log_max(&self) -> Option<i8> { None }
    fn cdf(&self, symbol: u8) -> Prob {
        //println_stderr!("cdf for {:x} have {:x}", symbol, self.nibble);
        self.cdf[symbol as usize]
    }
    fn valid(&self) -> bool {
        true
    }
}

impl CDF16 for ExternalProbCDF16 {
    fn average(&self, other:&Self, mix_rate:i32) -> Self {
        if self.max() < 64 && other.max() > 64 {
             //return other.clone();
        }
        if self.max() > 64 && other.max() < 64 {
             //return self.clone();
        }
        if self.entropy() > other.entropy() {
             //return other.clone();
        }
        //return self.clone();
        let mut retval = *self;
        let ourmax = i64::from(self.max());
        let othermax = i64::from(other.max());
        let maxmax = core::cmp::min(ourmax, othermax);
        let lgmax = 64 - maxmax.leading_zeros();
        let inv_mix_rate = (1 << BLEND_FIXED_POINT_PRECISION) - mix_rate;
        for (s, o) in retval.cdf.iter_mut().zip(other.cdf.iter()) {
            *s = (((i64::from(*s) * i64::from(mix_rate) *othermax + i64::from(*o) * i64::from(inv_mix_rate) * ourmax + 1) >> BLEND_FIXED_POINT_PRECISION) >> lgmax) as Prob;
        }
        retval
    }
    fn blend(&mut self, symbol: u8, speed: Speed) {
        return;
    }
}
