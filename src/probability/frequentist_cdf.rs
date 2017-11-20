use core;
use super::interface::{Prob, BaseCDF, Speed, CDF16, BLEND_FIXED_POINT_PRECISION};
fn to_bit_i32(val: i32, shift_val: u8) -> u32 {
    if val != 0 {
        1 << shift_val
    } else {
        0
    }
}


fn movemask_epi8_i32(data:[i32;8]) -> u32{
    to_bit_i32(data[0] & 0x80 , 0) |
    to_bit_i32(data[0] & 0x8000 , 1) |
    to_bit_i32(data[0] & 0x800000 , 2) |
    to_bit_i32(data[0] & -0x80000000, 3) |

    to_bit_i32(data[1] & 0x80 , 4) |
    to_bit_i32(data[1] & 0x8000 , 5) |
    to_bit_i32(data[1] & 0x800000 , 6) |
    to_bit_i32(data[1] & -0x80000000, 7) |

    to_bit_i32(data[2] & 0x80 , 8) |
    to_bit_i32(data[2] & 0x8000 , 9) |
    to_bit_i32(data[2] & 0x800000 , 10) |
    to_bit_i32(data[2] & -0x80000000, 11) |

    to_bit_i32(data[3] & 0x80 , 12) |
    to_bit_i32(data[3] & 0x8000 , 13) |
    to_bit_i32(data[3] & 0x800000 , 14) |
    to_bit_i32(data[3] & -0x80000000, 15) |

    to_bit_i32(data[4] & 0x80 , 16) |
    to_bit_i32(data[4] & 0x8000 , 17) |
    to_bit_i32(data[4] & 0x800000 , 18) |
    to_bit_i32(data[4] & -0x80000000, 19) |

    to_bit_i32(data[5] & 0x80 , 20) |
    to_bit_i32(data[5] & 0x8000 , 21) |
    to_bit_i32(data[5] & 0x800000 , 22) |
    to_bit_i32(data[5] & -0x80000000, 23) |

    to_bit_i32(data[6] & 0x80 , 24) |
    to_bit_i32(data[6] & 0x8000 , 25) |
    to_bit_i32(data[6] & 0x800000 , 26) |
    to_bit_i32(data[6] & -0x80000000, 27) |

    to_bit_i32(data[7] & 0x80 , 28) |
    to_bit_i32(data[7] & 0x8000 , 29) |
    to_bit_i32(data[7] & 0x800000 , 30) |
    to_bit_i32(data[7] & -0x80000000, 31)
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

#[allow(unused)]
macro_rules! each16{
    ($src0: expr, $func: expr) => {
    [$func($src0[0]),
     $func($src0[1]),
     $func($src0[2]),
     $func($src0[3]),
     $func($src0[4]),
     $func($src0[5]),
     $func($src0[6]),
     $func($src0[7]),
     $func($src0[8]),
     $func($src0[9]),
     $func($src0[10]),
     $func($src0[11]),
     $func($src0[12]),
     $func($src0[13]),
     $func($src0[14]),
     $func($src0[15]),
    ]
    }
}
#[allow(unused)]
macro_rules! set1 {
    ($src: expr, $val: expr) =>{
        [$val; 16]
    }
}

fn srl(a:Prob) -> Prob {
    a >> 1
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

impl CDF16 for FrequentistCDF16 {
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
        *s = (((i64::from(*s) * i64::from(mix_rate) * othermax + i64::from(*o) * i64::from(inv_mix_rate) * ourmax + 1) >> BLEND_FIXED_POINT_PRECISION) >> lgmax) as Prob;
        }
        retval
    }
    fn blend(&mut self, symbol: u8, speed: Speed) {
        const CDF_BIAS : [Prob;16] = [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16];
        let increment : Prob =
            match speed {
                Speed::GEOLOGIC => 2,
                Speed::GLACIAL => 4,
                Speed::MUD => 16,
                Speed::SLOW => 32,
                Speed::MED => 48,
                Speed::FAST => 96,
                Speed::PLANE => 128,
                Speed::ROCKET => 384,
            };
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
