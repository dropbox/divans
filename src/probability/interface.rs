use core;
pub type Prob = i16; // can be i32
pub const MAX_FREQUENTIST_PROB: Prob = 0x4000;
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
                v_f64 * (-v_f64.log2())
            };
        }
        sum
    }
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
    fn rescaled_cdf(&self, sym: u8) -> i32 {
        i32::from(self.cdf(sym)) << LOG2_SCALE
    }
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
#[derive(Clone)]
#[repr(u16)]
pub enum Speed {
    GEOLOGIC = 2,
    GLACIAL = 4,
    MUD = 16,
    SLOW = 32,
    MED = 48,
    FAST = 96,
    PLANE = 128,
    ROCKET = 384,
}

impl core::str::FromStr for Speed {
    type Err = ();
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
            _ => Err(()),
        }
    }
}

pub trait CDF16: Sized + Default + Copy + BaseCDF {
    fn blend(&mut self, symbol: u8, dyn:Speed);

    // TODO: this convenience function should probably live elsewhere.
    fn float_array(&self) -> [f32; 16] {
        let mut ret = [0.0f32; 16];
        for (i, ret_item) in ret.iter_mut().enumerate() {
            *ret_item = f32::from(self.cdf(i as u8)) / f32::from(self.max());
       }
        ret
    }
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
        self.cost += -p.log2();
        match self.true_entropy() {
            None => {},
            Some(e) => { self.rolling_entropy_sum += e; }
        }
        self.cdf.blend(symbol, speed);
    }
    fn float_array(&self) -> [f32; 16] { self.cdf.float_array() }
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
                    p * (-p.log2())
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

