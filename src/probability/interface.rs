use core;
pub type Prob = i16; // can be i32
pub const
MAX_FREQUENTIST_PROB: Prob = 0xa00;
#[cfg(feature="billing")]
use std::io::Write;
#[cfg(feature="billing")]
macro_rules! println_stderr(
    ($($val:tt)*) => { {
//        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);

#[cfg(not(feature="billing"))]
macro_rules! println_stderr(
    ($($val:tt)*) => { {
//        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);

#[derive(Copy,Clone,PartialEq,Eq,Debug)]
pub struct ProbRange {
    pub start: Prob,
    pub freq: Prob,
}

#[derive(Copy,Clone,PartialEq,Eq,Debug)]
pub struct SymStartFreq {
    pub range: ProbRange,
    pub sym: u8,
}

#[cfg(not(feature="no-stdlib"))]
fn log2(x:f64) -> f64 {
    x.log2()
}

#[cfg(feature="no-stdlib")]
fn log2(x:f64) -> f64 {
    (63 - (x as u64).leading_zeros()) as f64 // hack
}

// Common interface for CDF2 and CDF16, with optional methods.
pub trait BaseCDF {

    // the cardinality of symbols supported. Typical implementation values are 2 and 16.
    fn num_symbols() -> u8;

    // the cumulative distribution function evaluated at the given symbol.
    fn cdf(&self, symbol: u8) -> Prob;

    // the probability distribution function evaluated at the given symbol.
    fn pdf(&self, symbol: u8) -> Prob {
        debug_assert!(symbol < Self::num_symbols());
        if symbol == 0 {
            self.cdf(symbol)
        } else {
            self.cdf(symbol) - self.cdf(symbol - 1)
        }
    }
    fn div_by_max(&self, val: i32) -> i32;
    // the maximum value relative to which cdf() and pdf() values should be normalized.
    fn max(&self) -> Prob;

    // the base-2 logarithm of max(), if available, to support bit-shifting.
    fn log_max(&self) -> Option<i8>;

    // returns true if used.
    fn used(&self) -> bool { false }

    // returns true if valid.
    fn valid(&self) -> bool { false }

    // returns the entropy of the current distribution.
    fn entropy(&self) -> f64 {
        let mut sum = 0.0f64;
        for i in 0..Self::num_symbols() {
            let v = self.pdf(i as u8);
            sum += if v == 0 { 0.0f64 } else {
                let v_f64 = f64::from(v) / f64::from(self.max());
                v_f64 * (log2(-v_f64))
            };
        }
        sum
    }
    #[inline(always)]
    fn sym_to_start_and_freq(&self,
                             sym: u8) -> SymStartFreq {
        let cdf_prev = if sym != 0 {self.div_by_max(i32::from(self.cdf(sym - 1)) << LOG2_SCALE)} else { 0 };
        let cdf_sym = self.div_by_max((i32::from(self.cdf(sym)) << LOG2_SCALE));
        let freq = cdf_sym - cdf_prev;
        SymStartFreq {
            range: ProbRange {start: cdf_prev as Prob + 1, // major hax
                              freq:  freq as Prob - 1, // don't want rounding errors to work out unfavorably
            },
            sym: sym,
        }
    }
    #[inline(always)]
    fn rescaled_cdf(&self, sym: u8) -> i32 {
        i32::from(self.cdf(sym)) << LOG2_SCALE
    }
    #[inline(always)]
    fn cdf_offset_to_sym_start_and_freq(&self,
                                        cdf_offset_p: Prob) -> SymStartFreq {
        let rescaled_cdf_offset = ((i32::from(cdf_offset_p) * i32::from(self.max())) >> LOG2_SCALE) as i16;
        /* nice log(n) version which has too much dependent math, apparently, to be efficient
        let candidate0 = 7u8;
        let candidate1 = candidate0 - 4 + (((rescaled_cdf_offset >= self.cdf(candidate0)) as u8) << 3); // candidate1=3 or 11
        let candidate2 = candidate1 - 2 + (((rescaled_cdf_offset >= self.cdf(candidate1)) as u8) << 2); // candidate2=1,5,9 or 13
        let candidate3 = candidate2 - 1 + (((rescaled_cdf_offset >= self.cdf(candidate2)) as u8) << 1); // candidate3 or 12
        let final_decision = (rescaled_cdf_offset >= self.cdf(candidate3)) as u8;
        let sym = candidate3 + final_decision;
        self.sym_to_start_and_freq(sym)
         */
        //        let cdf15 = self.cdf(15);
        let sym: u8;
        if rescaled_cdf_offset < self.cdf(0) {
            sym = 0;
        } else if rescaled_cdf_offset < self.cdf(1) {
            sym = 1;
        } else if rescaled_cdf_offset < self.cdf(2) {
            sym = 2;
        } else if rescaled_cdf_offset < self.cdf(3) {
            sym = 3;
        } else if rescaled_cdf_offset < self.cdf(4) {
            sym = 4;
        } else if rescaled_cdf_offset < self.cdf(5) {
            sym = 5;
        } else if rescaled_cdf_offset < self.cdf(6) {
            sym = 6;
        } else if rescaled_cdf_offset < self.cdf(7) {
            sym = 7;
        } else if rescaled_cdf_offset < self.cdf(8) {
            sym = 8;
        } else if rescaled_cdf_offset < self.cdf(9) {
            sym = 9;
        } else if rescaled_cdf_offset < self.cdf(10) {
            sym = 10;
        } else if rescaled_cdf_offset < self.cdf(11) {
            sym = 11;
        } else if rescaled_cdf_offset < self.cdf(12) {
            sym = 12;
        } else if rescaled_cdf_offset < self.cdf(13) {
            sym = 13;
        } else if rescaled_cdf_offset < self.cdf(14) {
            sym = 14;
        } else {
            sym = 15;
        }
        return self.sym_to_start_and_freq(sym);
        /* // this really should be the same speed as above
        for i in 0..15 {
            if rescaled_cdf_offset < self.cdf(i as u8) {
                return self.sym_to_start_and_freq(i);
            }
        }
        self.sym_to_start_and_freq(15)
*/
    }

    // These methods are optional because implementing them requires nontrivial bookkeeping.
    // Only CDFs that are intended for debugging should support them.
    fn num_samples(&self) -> Option<u32> { None }
    fn true_entropy(&self) -> Option<f64> { None }
    fn rolling_entropy(&self) -> Option<f64> { None }
    fn encoding_cost(&self) -> Option<f64> { None }
    fn num_variants(&self) -> usize {
        0
    }
    fn variant_cost(&self, variant_index: usize) -> f32 {
        0.0
    }
    fn base_variant_cost(&self) -> f32 {
        0.0
    }
}

#[derive(Clone, Copy)]
pub struct CDF2 {
    counts: [u8; 2],
    pub prob: u8,
}

impl Default for CDF2 {
    fn default() -> Self {
        CDF2 {
            counts: [1, 1],
            prob: 128,
        }
    }
}

impl BaseCDF for CDF2 {
    fn cdf_offset_to_sym_start_and_freq(
        &self,
        cdf_offset: Prob) -> SymStartFreq {
        let bit = ((i32::from(cdf_offset) * i32::from(self.max())) >> LOG2_SCALE) >= i32::from(self.prob);
        let rescaled_prob = self.div_by_max(i32::from(self.prob) << LOG2_SCALE);
        SymStartFreq {
            sym: bit as u8,
            range: ProbRange {start: if bit {rescaled_prob as Prob} else {0},
                              freq: if bit {
                                  ((1 << LOG2_SCALE) - rescaled_prob) as Prob
                              } else {
                                  rescaled_prob as Prob
                              },
            }
        }
    }
    fn div_by_max(&self, val:i32) -> i32 {
        return val / i32::from(self.max())
    }
    fn num_symbols() -> u8 { 2 }
    fn cdf(&self, symbol: u8) -> Prob {
        match symbol {
            0 => Prob::from(self.prob),
            1 => 256,
            _ => { panic!("Symbol out of range"); }
        }
    }
    fn used(&self) -> bool {
        self.counts[0] != 1 || self.counts[1] != 1
    }
    fn max(&self) -> Prob {
        256
    }
    fn log_max(&self) -> Option<i8> {
        Some(8)
    }
}

impl CDF2 {
    pub fn blend(&mut self, symbol: bool, _speed: &Speed) {
        let fcount = self.counts[0];
        let tcount = self.counts[1];
        debug_assert!(fcount != 0);
        debug_assert!(tcount != 0);

        let obs = if symbol {1} else {0};
        let overflow = self.counts[obs] == 0xff;
        self.counts[obs] = self.counts[obs].wrapping_add(1);
        if overflow {
            let not_obs = if symbol {0} else {1};
            let neverseen = self.counts[not_obs] == 1;
            if neverseen {
                self.counts[obs] = 0xff;
                self.prob = if symbol {0} else {0xff};
            } else {
                self.counts[0] = ((1 + u16::from(fcount)) >> 1) as u8;
                self.counts[1] = ((1 + u16::from(tcount)) >> 1) as u8;
                self.counts[obs] = 129;
                self.prob = ((u16::from(self.counts[0]) << 8) / (u16::from(self.counts[0]) + u16::from(self.counts[1]))) as u8;
            }
        } else {
            self.prob = ((u16::from(self.counts[0]) << 8) / (u16::from(fcount) + u16::from(tcount) + 1)) as u8;
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Speed(i16,i16);
pub const SPEED_PALETTE_SIZE: usize = 15;
pub type SpeedPalette = [Speed;SPEED_PALETTE_SIZE];
impl Speed {
    pub const ENCODER_DEFAULT_PALETTE: SpeedPalette = [
        Speed(0, 1024),
        Speed(1, 32),
        Speed(1, 128),
        Speed(1, 16384),
        Speed(2, 1024),
        Speed(4, 1024),
        Speed(8, 8192),
        Speed(16, 48),
        Speed(16, 8192),// old mud
        Speed(32, 4096),
        Speed(64, 16384),
        Speed(128, 256),
        Speed(128, 16384),
        Speed(512, 16384),
        //Speed(1024, 16384),
        Speed(1664, 16384),
        ];
    pub const GEOLOGIC: Speed = Speed(0x0001, 0x4000);
    pub const GLACIAL: Speed = Speed(0x0004, 0x0a00);
    pub const MUD: Speed =   Speed(0x0010, 0x2000);
    pub const SLOW: Speed =  Speed(0x0020, 0x1000);
    pub const MED: Speed =   Speed(0x0030, 0x4000);
    pub const FAST: Speed =  Speed(0x0060, 0x4000);
    pub const PLANE: Speed = Speed(0x0080, 0x4000);
    pub const ROCKET: Speed =Speed(0x0180, 0x4000);
    pub fn to_f8_tuple(&self) -> (u8, u8) {
        (speed_to_u8(self.inc()), speed_to_u8(self.lim()))
    }
    pub fn from_f8_tuple(inp: (u8, u8)) -> Self {
        Speed::new(u8_to_speed(inp.0), u8_to_speed(inp.1))
    }
    #[inline(never)]
    #[cold]
    pub fn cold_new(inc:i16, max: i16) -> Speed {
        Self::new(inc, max)
    }
    #[inline(always)]
    pub fn new(inc:i16, max: i16) -> Speed {
        debug_assert!(inc <= 0x4000); // otherwise some sse hax fail
        debug_assert!(max <= 0x4000); // otherwise some sse hax fail
        Speed(inc, max)
    }
    #[inline(always)]
    pub fn lim(&self) -> i16 {
        let ret = self.1;
        debug_assert!(ret <= 0x4000); // otherwise some sse hax fail
        ret
    }
    #[inline(always)]
    pub fn inc_and_gets(&mut self, ander: i16) {
        self.0 &= ander;
    }
    #[inline(always)]
    pub fn lim_or_gets(&mut self, orer: i16) {
        self.1 |= orer;
    }
    #[inline(always)]
    pub fn inc(&self) -> i16 {
        self.0
    }
    #[inline(always)]
    pub fn set_lim(&mut self, lim: i16) {
        debug_assert!(lim <= 0x4000); // otherwise some sse hax fail
        self.1 = lim;
    }
    #[inline(always)]
    pub fn set_inc(&mut self, inc: i16) {
        debug_assert!(inc <= 0x4000);
        self.0 = inc;
    }
}
impl core::str::FromStr for Speed {
    type Err = core::num::ParseIntError;
    fn from_str(inp:&str) -> Result<Speed, Self::Err> {
        match inp {
            "GEOLOGIC" => Ok(Speed::GEOLOGIC),
            "GLACIAL" => Ok(Speed::GLACIAL),
            "MUD" => Ok(Speed::MUD),
            "SLOW" => Ok(Speed::SLOW),
            "MED" => Ok(Speed::MED),
            "FAST" => Ok(Speed::FAST),
            "PLANE" => Ok(Speed::PLANE),
            "ROCKET" => Ok(Speed::ROCKET),
            _ => {
               let mut split_location = 0;
               for (index, item) in inp.chars().enumerate() {
                  if item == ',' {
                     split_location = index;
                     break
                  }
               }
               let first_num_str = inp.split_at(split_location).0;
               let second_num_str = inp.split_at(split_location + 1).1;
               let conv_inc = u16::from_str(first_num_str);
               let conv_lim = u16::from_str(second_num_str);
               match conv_inc {
                  Err(e) => return Err(e),
                  Ok(inc) => match conv_lim {
                     Err(e) => return Err(e),
                     Ok(lim) => {
                         if lim <= 16384 && inc < 16384 {
                         return Ok(Speed::new(inc as i16, lim as i16));
                         } else {
                             match "65537".parse::<u16>() {
                                Err(e) => return Err(e),
                                Ok(f) => unreachable!(),
                             }
                         }
                     },
                  },
               }
            },
        }
    }
}

pub trait CDF16: Sized + Default + Copy + BaseCDF {
    fn blend(&mut self, symbol: u8, dyn:Speed);
    fn average(&self, other: &Self, mix_rate: i32) ->Self;
}

pub const BLEND_FIXED_POINT_PRECISION : i8 = 15;
pub const CDF_BITS : usize = 15; // 15 bits
pub const LOG2_SCALE: u32 = CDF_BITS as u32;
pub const CDF_MAX : Prob = 32_767; // last value is implicitly 32768
const CDF_LIMIT : i64 = (CDF_MAX as i64) + 1;




#[allow(unused)]
fn gt(a:Prob, b:Prob) -> Prob {
    (-((a > b) as i64)) as Prob
}
#[allow(unused)]
fn gte_bool(a:Prob, b:Prob) -> Prob {
    (a >= b) as Prob
}



#[cfg(feature="debug_entropy")]
#[derive(Clone,Copy,Default)]
pub struct DebugWrapperCDF16<Cdf16: CDF16> {
    pub cdf: Cdf16,
    pub counts: [u32; 16],
    cost: f64,
    rolling_entropy_sum: f64
}

#[cfg(feature="debug_entropy")]
impl<Cdf16> CDF16 for DebugWrapperCDF16<Cdf16> where Cdf16: CDF16 {
    fn blend(&mut self, symbol: u8, speed: Speed) {
        self.counts[symbol as usize] += 1;
        let p = self.cdf.pdf(symbol) as f64 / self.cdf.max() as f64;
        self.cost += -log2(p);
        match self.true_entropy() {
            None => {},
            Some(e) => { self.rolling_entropy_sum += e; }
        }
        self.cdf.blend(symbol, speed);
    }
    fn average(&self, other: &Self, mix_rate: i32) -> Self {
        // NOTE(jongmin): The notion of averaging for a debug CDF is not well-formed
        // because its private fields depend on the blend history that's not preserved in averaging.
        let mut counts_both = [0u32; 16];
        for i in 0..16 {
            counts_both[i] = self.counts[i] + other.counts[i];
        }
        Self {
            cdf: self.cdf.average(&other.cdf, mix_rate),
            counts: counts_both,
            cost: (self.cost + other.cost),
            rolling_entropy_sum: (self.rolling_entropy_sum + other.rolling_entropy_sum)
        }
    }
}

#[cfg(feature="debug_entropy")]
impl<Cdf16> BaseCDF for DebugWrapperCDF16<Cdf16> where Cdf16: CDF16 + BaseCDF {
    fn num_symbols() -> u8 { 16 }
    fn cdf(&self, symbol: u8) -> Prob { self.cdf.cdf(symbol) }
    fn pdf(&self, symbol: u8) -> Prob { self.cdf.pdf(symbol) }
    fn max(&self) -> Prob { self.cdf.max() }
    fn log_max(&self) -> Option<i8> { self.cdf.log_max() }
    fn entropy(&self) -> f64 { self.cdf.entropy() }
    fn valid(&self) -> bool { self.cdf.valid() }
    fn div_by_max(&self, val: i32) -> i32 {self.cdf.div_by_max(val)}
    fn used(&self) -> bool {
        self.num_samples().unwrap() > 0
    }

    fn num_samples(&self) -> Option<u32> {
        let mut sum : u32 = 0;
        for i in 0..16 {
            sum += self.counts[i];
        }
        Some(sum)
    }
    fn true_entropy(&self) -> Option<f64> {
        let num_samples = self.num_samples().unwrap();
        if num_samples > 0 {
            let mut sum : f64 = 0.0;
            for i in 0..16 {
                sum += if self.counts[i] == 0 { 0.0f64 } else {
                    let p = (self.counts[i] as f64) / (num_samples as f64);
                    p * (log2(-p))
                };
            }
            Some(sum)
        } else {
            None
        }
    }
    fn rolling_entropy(&self) -> Option<f64> {
        match self.num_samples() {
            None => None,
            Some(n) => Some(self.rolling_entropy_sum / n as f64)
        }
    }
    fn encoding_cost(&self) -> Option<f64> {
        Some(self.cost)
    }
}

#[cfg(feature="debug_entropy")]
impl<Cdf16> DebugWrapperCDF16<Cdf16> where Cdf16: CDF16 {
    fn new(cdf: Cdf16) -> Self {
        DebugWrapperCDF16::<Cdf16> {
            cdf: cdf,
            counts: [0; 16],
            cost: 0.0,
            rolling_entropy_sum: 0.0
        }
    }
}

#[cfg(test)]
#[cfg(feature="debug_entropy")]
mod test {
    use super::{BaseCDF, CDF16, Speed};
    use super::super::{DebugWrapperCDF16, FrequentistCDF16, };
    type DebugWrapperCDF16Impl = DebugWrapperCDF16<FrequentistCDF16>;
    declare_common_tests!(DebugWrapperCDF16Impl);

    #[test]
    fn test_debug_info() {
        let mut wrapper_cdf = DebugWrapperCDF16::<FrequentistCDF16>::default();
        let mut reference_cdf = FrequentistCDF16::default();
        let num_samples = 1234usize;
        for i in 0..num_samples {
            wrapper_cdf.blend((i & 0xf) as u8, Speed::MED);
            reference_cdf.blend((i & 0xf) as u8, Speed::MED);
        }
        assert!(wrapper_cdf.num_samples().is_some());
        assert_eq!(wrapper_cdf.num_samples().unwrap(), num_samples as u32);

        use super::super::common_tests;
        common_tests::assert_cdf_eq(&reference_cdf, &wrapper_cdf.cdf);
    }
}
pub fn speed_to_u8(data: i16) -> u8 {
    let length = 16 - data.leading_zeros() as u8;
    let mantissa = if data != 0 {
        let rem = data - (1 << (length - 1));
        (rem << 3) >> (length - 1)
    } else {
        0
    };
    (length << 3) | mantissa as u8
}

pub fn u8_to_speed(data: u8) -> i16 {
    if data < 8 {
        0
    } else {
        let log_val = (data >> 3) - 1;
        let rem = (i16::from(data) & 0x7) << log_val;
        (1i16 << log_val) | (rem >> 3)
    }
}
#[cfg(test)]
mod test {
    use super::speed_to_u8;
    use super::u8_to_speed;
    fn tst_u8_to_speed(data: i16) {
        assert_eq!(u8_to_speed(speed_to_u8(data)), data);
    }
    #[test]
    fn test_u8_to_speed() {
        tst_u8_to_speed(0);
        tst_u8_to_speed(1);
        tst_u8_to_speed(2);
        tst_u8_to_speed(3);
        tst_u8_to_speed(4);
        tst_u8_to_speed(5);
        tst_u8_to_speed(6);
        tst_u8_to_speed(7);
        tst_u8_to_speed(8);
        tst_u8_to_speed(10);
        tst_u8_to_speed(12);
        tst_u8_to_speed(16);
        tst_u8_to_speed(24);
        tst_u8_to_speed(32);
        tst_u8_to_speed(48);
        tst_u8_to_speed(64);
        tst_u8_to_speed(96);
        tst_u8_to_speed(768);
        tst_u8_to_speed(1280);
        tst_u8_to_speed(1536);
        tst_u8_to_speed(1664);
    }
}
