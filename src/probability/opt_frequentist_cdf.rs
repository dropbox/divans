use core;
use super::interface::{Prob, BaseCDF, Speed, CDF16, BLEND_FIXED_POINT_PRECISION, LOG2_SCALE, CDF_BITS};
use super::frequentist_cdf::FrequentistCDF16;
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
pub struct OptFrequentistCDF16 {
    pub cdf: FrequentistCDF16,
    pub inv_max: i64,
    pub cdf_max_bitlen: u8,
}

impl OptFrequentistCDF16 {
    fn new(input:FrequentistCDF16) -> Self {
        let (inv_max, cdf_max_bitlen) = compute_divisor(input.max());
        OptFrequentistCDF16{
            cdf:input,
            inv_max: inv_max,
            cdf_max_bitlen: cdf_max_bitlen,
        }
    }
}

impl Default for OptFrequentistCDF16 {
    fn default() -> Self {
        Self::new(FrequentistCDF16::default())
    }
}


impl BaseCDF for OptFrequentistCDF16 {
    fn num_symbols() -> u8 { 16 }
    fn used(&self) -> bool {
        self.cdf.used()
    }
    fn max(&self) -> Prob {
        self.cdf.max()
    }
    fn log_max(&self) -> Option<i8> { None }
    fn cdf(&self, symbol: u8) -> Prob {
        self.cdf.cdf(symbol)
    }
    fn valid(&self) -> bool {
        let (inv_max, cdf_max_bitlen) = compute_divisor(self.max());
        if self.inv_max != inv_max || self.cdf_max_bitlen != cdf_max_bitlen {
           return false;
        }
        self.cdf.valid()
    }
    fn div_by_max(&self, num: i32) -> i32 {
        let idiv_mul_num = i64::from(self.inv_max) * i64::from(num);
         ((idiv_mul_num >> LOG_MAX_NUMERATOR) as i32
             + (((i64::from(num) - (idiv_mul_num >> LOG_MAX_NUMERATOR)) as i32) >> 1))
          >> self.cdf_max_bitlen
    }
}

fn k16bit_length(d:i16) -> u8 {
    (16 - d.leading_zeros()) as u8
}
const LOG_MAX_NUMERATOR: usize = LOG2_SCALE as usize + CDF_BITS;
fn compute_divisor(d: i16) -> (i64, u8) {
    let bit_len = k16bit_length(d);
    (((((( 1i64 << bit_len) - i64::from(d)) << (LOG_MAX_NUMERATOR))) / i64::from(d)) + 1, bit_len.wrapping_sub(1))
}

impl CDF16 for OptFrequentistCDF16 {
    fn average(&self, other:&Self, mix_rate:i32) -> Self {
        let ret = self.cdf.average(&other.cdf, mix_rate);
        Self::new(ret)
    }
    fn blend(&mut self, symbol: u8, speed: Speed) {
        self.cdf.blend(symbol, speed);
        let (inv_max, cdf_max_bitlen) = compute_divisor(self.max());
        self.inv_max = inv_max;
        self.cdf_max_bitlen = cdf_max_bitlen;
    }
}
