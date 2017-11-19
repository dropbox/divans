use core;
pub type Prob = i16; // can be i32
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


pub struct SymStartFreq {
    pub start: Prob,
    pub freq: Prob,
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
        let cdf_prev = if sym != 0 {(i32::from(self.cdf(sym - 1)) << LOG2_SCALE) / i32::from(self.max())} else { 0 };
        let cdf_sym = (i32::from(self.cdf(sym)) << LOG2_SCALE) / i32::from(self.max());
        let freq = cdf_sym - cdf_prev;
        SymStartFreq {
            start: cdf_prev as Prob + 1, // major hax
            freq:  freq as Prob - 1, // don't want rounding errors to work out unfavorably
            sym: sym,
        }
    }
    fn rescaled_cdf(&self, sym: u8) -> i32 {
        i32::from(self.cdf(sym)) << LOG2_SCALE
    }
    fn cdf_offset_to_sym_start_and_freq(&self,
                                        cdf_offset_p: Prob) -> SymStartFreq {
        /*
        for i in 0..16 {
            let cdf_cur = (i32::from(self.cdf(i as u8))<<LOG2_SCALE) / i32::from(self.max());
            if i32::from(cdf_offset_p) < cdf_cur {
                let cdf_prev = if i != 0 {
                    (i32::from(self.cdf(i as u8 - 1))<<LOG2_SCALE) / i32::from(self.max())
                } else {
                    0
                };
                return SymStartFreq{
                    sym: i as u8,
                    start: cdf_prev as Prob,
                    freq: (cdf_cur - cdf_prev) as Prob,
                };
            }
        }
        panic!("unreachable due to max value of cdf");
         */
        let rescaled_cdf_offset = ((i32::from(cdf_offset_p) * i32::from(self.max())) >> LOG2_SCALE) as i16;
        let symbol_less = [
            -((rescaled_cdf_offset >= self.cdf(0)) as i16),
            -((rescaled_cdf_offset >= self.cdf(1)) as i16),
            -((rescaled_cdf_offset >= self.cdf(2)) as i16),
            -((rescaled_cdf_offset >= self.cdf(3)) as i16),
            -((rescaled_cdf_offset >= self.cdf(4)) as i16),
            -((rescaled_cdf_offset >= self.cdf(5)) as i16),
            -((rescaled_cdf_offset >= self.cdf(6)) as i16),
            -((rescaled_cdf_offset >= self.cdf(7)) as i16),
            -((rescaled_cdf_offset >= self.cdf(8)) as i16),
            -((rescaled_cdf_offset >= self.cdf(9)) as i16),
            -((rescaled_cdf_offset >= self.cdf(10)) as i16),
            -((rescaled_cdf_offset >= self.cdf(11)) as i16),
            -((rescaled_cdf_offset >= self.cdf(12)) as i16),
            -((rescaled_cdf_offset >= self.cdf(13)) as i16),
            -((rescaled_cdf_offset >= self.cdf(14)) as i16),
            -((rescaled_cdf_offset >= self.cdf(15)) as i16),
            ];
        let bitmask:u32 = movemask_epi8(symbol_less);
        let symbol_id = ((32 - u32::from(bitmask).leading_zeros()) >> 1) as u8;
        self.sym_to_start_and_freq(symbol_id)
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
        let rescaled_prob = (i32::from(self.prob) << LOG2_SCALE) / i32::from(self.max());
        SymStartFreq {
            sym: bit as u8,
            start: if bit {rescaled_prob as Prob} else {0},
            freq: if bit {((1 << LOG2_SCALE) - rescaled_prob) as Prob} else {rescaled_prob as Prob}
        }
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
#[repr(C)]
pub enum Speed {
    GEOLOGIC,
    GLACIAL,
    MUD,
    SLOW,
    MED,
    FAST,
    PLANE,
    ROCKET,
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
const CDF_BITS : usize = 15; // 15 bits
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

fn movemask_epi8(data:[i16;16]) -> u32{
    to_bit_u32(data[0] & -0x8000 , 0) |
    to_bit_u32(data[0] & 0x80 , 1) |
    to_bit_u32(data[1] & -0x8000 , 2) |
    to_bit_u32(data[1] & 0x80 , 3) |
    to_bit_u32(data[2] & -0x8000 , 4) |
    to_bit_u32(data[2] & 0x80 , 5) |
    to_bit_u32(data[3] & -0x8000 , 6) |
    to_bit_u32(data[3] & 0x80 , 7) |
    to_bit_u32(data[4] & -0x8000 , 8) |
    to_bit_u32(data[4] & 0x80 , 9) |
    to_bit_u32(data[5] & -0x8000 , 10) |
    to_bit_u32(data[5] & 0x80 , 11) |
    to_bit_u32(data[6] & -0x8000 , 12) |
    to_bit_u32(data[6] & 0x80 , 13) |
    to_bit_u32(data[7] & -0x8000 , 14) |
    to_bit_u32(data[7] & 0x80 , 15) |
    to_bit_u32(data[8] & -0x8000 , 16) |
    to_bit_u32(data[8] & 0x80 , 17) |
    to_bit_u32(data[9] & -0x8000 , 18) |
    to_bit_u32(data[9] & 0x80 , 19) |
    to_bit_u32(data[10] & -0x8000 , 20) |
    to_bit_u32(data[10] & 0x80 , 21) |
    to_bit_u32(data[11] & -0x8000 , 22) |
    to_bit_u32(data[11] & 0x80 , 23) |
    to_bit_u32(data[12] & -0x8000 , 24) |
    to_bit_u32(data[12] & 0x80 , 25) |
    to_bit_u32(data[13] & -0x8000 , 26) |
    to_bit_u32(data[13] & 0x80 , 27) |
    to_bit_u32(data[14] & -0x8000 , 28) |
    to_bit_u32(data[14] & 0x80 , 29) |
    to_bit_u32(data[15] & -0x8000 , 30) |
    to_bit_u32(data[15] & 0x80 , 31)
}

fn to_bit_u32(val: i16, shift_val: u8) -> u32 {
    if val != 0 {
        1 << shift_val
    } else {
        0
    }
}
