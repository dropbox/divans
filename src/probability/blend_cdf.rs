use super::interface::{Prob, BaseCDF, Speed, CDF_MAX, CDF16, BLEND_FIXED_POINT_PRECISION};


#[allow(unused)]
fn gte(a:Prob, b:Prob) -> Prob {
    (-((a >= b) as i64)) as Prob
}
fn and(a:Prob, b:Prob) -> Prob {
    a & b
}
fn add(a:Prob, b:Prob) -> Prob {
    a.wrapping_add(b)
}

pub fn mul_blend(baseline: [Prob;16], to_blend: [Prob;16], blend : i32, bias : i32) -> [Prob;16] {
    const SCALE :i32 = 1i32 << BLEND_FIXED_POINT_PRECISION;
    let mut epi32:[i32;8] = [i32::from(to_blend[0]),
                             i32::from(to_blend[1]),
                             i32::from(to_blend[2]),
                             i32::from(to_blend[3]),
                             i32::from(to_blend[4]),
                             i32::from(to_blend[5]),
                             i32::from(to_blend[6]),
                             i32::from(to_blend[7])];
    let scale_minus_blend = SCALE - blend;
    for i in 0..8 {
        epi32[i] *= blend;
        epi32[i] += i32::from(baseline[i]) * scale_minus_blend + bias;
        epi32[i] >>= BLEND_FIXED_POINT_PRECISION;
    }
    let mut retval : [Prob;16] =[epi32[0] as Prob,
                                 epi32[1] as Prob,
                                 epi32[2] as Prob,
                                 epi32[3] as Prob,
                                 epi32[4] as Prob,
                                 epi32[5] as Prob,
                                 epi32[6] as Prob,
                                 epi32[7] as Prob,
                                 0,0,0,0,0,0,0,0];
    let mut epi32:[i32;8] = [i32::from(to_blend[8]),
                             i32::from(to_blend[9]),
                             i32::from(to_blend[10]),
                             i32::from(to_blend[11]),
                             i32::from(to_blend[12]),
                             i32::from(to_blend[13]),
                             i32::from(to_blend[14]),
                             i32::from(to_blend[15])];
    for i in 8..16 {
        epi32[i - 8] *= blend;
        epi32[i - 8] += i32::from(baseline[i]) * scale_minus_blend + bias;
        retval[i] = (epi32[i - 8] >> BLEND_FIXED_POINT_PRECISION) as Prob;
    }
    retval
}

macro_rules! each16bin {
    ($src0 : expr, $src1 : expr, $func: expr) => {
    [$func($src0[0], $src1[0]),
           $func($src0[1], $src1[1]),
           $func($src0[2], $src1[2]),
           $func($src0[3], $src1[3]),
           $func($src0[4], $src1[4]),
           $func($src0[5], $src1[5]),
           $func($src0[6], $src1[6]),
           $func($src0[7], $src1[7]),
           $func($src0[8], $src1[8]),
           $func($src0[9], $src1[9]),
           $func($src0[10], $src1[10]),
           $func($src0[11], $src1[11]),
           $func($src0[12], $src1[12]),
           $func($src0[13], $src1[13]),
           $func($src0[14], $src1[14]),
           $func($src0[15], $src1[15])]
    }
}
pub fn to_blend(symbol: u8) -> [Prob;16] {
    // The returned distribution has a max of DEL = CDF_MAX - 16, which guarantees that
    // by mixing only such distributions, we'll have at least 16 as the bias weight,
    // which is required to guarantee nonzero PDF everywhere.
    const CDF_INDEX : [Prob;16] = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15];
    const DEL: Prob = CDF_MAX - 16;
    let symbol16 = [Prob::from(symbol); 16];
    let delta16 = [DEL; 16];
    let mask_symbol = each16bin!(CDF_INDEX, symbol16, gte);
    each16bin!(delta16, mask_symbol, and)
}

pub fn to_blend_lut(symbol: u8) -> [Prob;16] {
    const DEL: Prob = CDF_MAX - 16;
    static CDF_SELECTOR : [[Prob;16];16] = [
        [DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,0,0,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,DEL as Prob]];
    CDF_SELECTOR[symbol as usize]
}
#[derive(Clone,Copy)]
pub struct BlendCDF16 {
    pub cdf: [Prob; 16],
    mix_rate: i32,
    count: i32,
}

impl BlendCDF16 {
    fn blend_internal(&mut self, to_blend: [Prob;16], mix_rate: i32) {
        self.cdf = mul_blend(self.cdf, to_blend, mix_rate, (self.count & 0xf) << (BLEND_FIXED_POINT_PRECISION - 4));
        if self.cdf[15] < (CDF_MAX - 16) - (self.cdf[15] >> 1) {
            for i in 0..16 {
                self.cdf[i] += self.cdf[i] >> 1;
            }
        }
        debug_assert!(self.cdf[15] <= CDF_MAX - 16);

    }
}
impl Default for BlendCDF16 {
    fn default() -> Self {
        BlendCDF16 {
            cdf: [0; 16],
            mix_rate: (1 << 10) + (1 << 9),
            count: 0,
        }
    }
}


impl BaseCDF for BlendCDF16 {
    fn num_symbols() -> u8 { 16 }
    fn used(&self) -> bool {
        for i in 0..16 {
            if self.cdf[i] > 0 {
                return true;
            }
        }
        false
    }
    fn max(&self) -> Prob {
        CDF_MAX as Prob
    }
    fn log_max(&self) -> Option<i8> {
        Some(15)
    }
    fn div_by_max(&self, val:i32) -> i32 {
        return val>>self.log_max().unwrap()
    }
    fn cdf(&self, symbol: u8) -> Prob {
        match symbol {
            15 => self.max(),
            _ => {
                // We want self.cdf[15] to be normalized to CDF_MAX, so take the difference to
                // be the latent bias term coming from a uniform distribution.
                let bias = CDF_MAX - self.cdf[15] as i16;
                debug_assert!(bias >= 16);
                self.cdf[symbol as usize] as Prob + ((i32::from(bias) * (i32::from(symbol + 1))) >> 4) as Prob
            }
        }
    }
    fn valid(&self) -> bool {
        for item in &self.cdf {
            if *item < 0 || !(*item <= CDF_MAX) {
                return false;
            }
        }
        true
    }
}

impl CDF16 for BlendCDF16 {
    fn average(&self, other: &Self, mix_rate: i32) ->Self {
        let mut retval = *self;
        retval.blend_internal(other.cdf, mix_rate);
        retval
    }
    fn blend(&mut self, symbol:u8, speed: Speed) {
        self.count = self.count.wrapping_add(1);
        let _mix_rate = match speed {
            Speed::GEOLOGIC => 32,
            Speed::GLACIAL => 64,
            Speed::MUD => 128,
            Speed::SLOW => 192,
            Speed::MED => 256,
            Speed::FAST => 384,
            Speed::PLANE => 512,
            Speed::ROCKET => 1100,
            a => a.inc(),
        };
        let to_blend = to_blend_lut(symbol);
        let mr = self.mix_rate;
        self.blend_internal(to_blend, mr);
        // Reduce the weight of bias in the first few iterations.
        self.mix_rate -= self.mix_rate >> 7;
        // NOTE(jongmin): geometrically decay mix_rate until it dips below 1 << 7;


    }
}

#[cfg(test)]
mod test {
    use super::{BlendCDF16, to_blend, to_blend_lut};
    declare_common_tests!(BlendCDF16);

    #[test]
    fn test_blend_lut() {
        for i in 0..16 {
            let a = to_blend(i as u8);
            let b = to_blend_lut(i as u8);
            for j in 0..16 {
                assert_eq!(a[j], b[j]);
            }
        }
    }

}
