use core;
use core::simd::FromBits;
use super::interface::{Prob, BaseCDF, Speed, CDF16, BLEND_FIXED_POINT_PRECISION, SymStartFreq, LOG2_SCALE};
use super::numeric;
use core::simd;
use core::simd::{i32x2, f64x2, i16x16, i64x4, i16x8, i8x32, i8x16, u32x8, u8x16, i64x2, i32x8};
//use stdsimd::vendor::__m256i;

#[derive(Clone,Copy)]
pub struct SIMDFrequentistCDF16 {
    pub cdf: i16x16,
}

impl SIMDFrequentistCDF16 {
    #[inline(always)]
    fn new(input: i16x16) -> Self {
        SIMDFrequentistCDF16 {
            cdf: input,
        }
    }
}

impl Default for SIMDFrequentistCDF16 {
    #[inline(always)]
    fn default() -> Self {
        SIMDFrequentistCDF16::new(i16x16::new(4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60, 64))
    }
}

extern "platform-intrinsic" {
    pub fn simd_shuffle4<T, U>(x: T, y: T, idx: [u32; 4]) -> U;
    pub fn simd_shuffle16<T, U>(x: T, y: T, idx: [u32; 16]) -> U;
}
#[cfg(feature="avoid-divide")]
#[inline(always)]
pub fn lookup_divisor(cdfmax: i16) -> (i64, u8) {
    numeric::lookup_divisor(cdfmax)
}
#[cfg(not(feature="avoid-divide"))]
#[inline(always)]
pub fn lookup_divisor(_cdfmax:i16) {
}

impl BaseCDF for SIMDFrequentistCDF16 {
    #[inline(always)]
    fn num_symbols() -> u8 { 16 }
    #[inline(always)]
    fn used(&self) -> bool { self.entropy() != Self::default().entropy() }
    #[inline(always)]
    fn max(&self) -> Prob { self.cdf.extract(15) }
    #[inline(always)]
    fn div_by_max(&self, val:i32) -> i32 {
        let divisor = self.cdf.extract(15) as u16;
        val / i32::from(divisor)
    }
    #[inline(always)]
    fn log_max(&self) -> Option<i8> { None }
    #[inline(always)]
    fn cdf(&self, symbol: u8) -> Prob {
        // bypass the internal assert by hinting to the compiler that symbol is 4-bit.
        self.cdf.extract(symbol as usize & 0xf)
    }
    fn valid(&self) -> bool {
        let mut slice = [0i16; 16];
        self.cdf.store_unaligned(&mut slice);
        for it in slice[0..15].iter().zip(slice[1..16].iter()) {
            let (prev, next) = it;
            if (*next <= *prev) {
                return false;
            }
        }
        true
    }
    #[inline(always)]
    #[cfg(not(feature="avoid-divide"))]
    fn sym_to_start_and_freq_with_div_hint(&self,
                                           sym: u8,
                                           inv_max_and_bitlen:()) -> SymStartFreq {
        self.sym_to_start_and_freq(sym)
    }

    #[inline(always)]
    fn sym_to_start_and_freq(&self,
                             sym: u8) -> SymStartFreq {
        let prev_cur = i32x2::new(if sym != 0 {self.cdf(sym - 1) as i32} else {0},
                                  self.cdf(sym) as i32);
        let scaled_prev_cur = f64x2::from(prev_cur << LOG2_SCALE);
        let prev_cur_over_max = scaled_prev_cur / (self.max() as f64);
        let start_end = i32x2::from(prev_cur_over_max);
        let start = start_end.extract(0) + 1; // the +1 is a major hax
        let freq = start_end.extract(1) - start; // don't want rounding errors to work out unfavorably
        SymStartFreq {
            range: super::interface::ProbRange{
                start: start as Prob,
                freq:  freq as Prob,
            },
            sym: sym,
        }
    }
    #[cfg(any(feature="portable-simd", not(target_arch="x86_64")))]
    #[inline(always)]
    fn cdf_offset_to_sym_start_and_freq(&self,
                                        cdf_offset_p: Prob) -> SymStartFreq {
        let cdfmax = self.max();
        let inv_max_and_bitlen = lookup_divisor(cdfmax);
        let rescaled_cdf_offset = ((i32::from(cdf_offset_p) * i32::from(cdfmax)) >> LOG2_SCALE) as i16;
        let symbol_less = i16x16::splat(rescaled_cdf_offset).ge(self.cdf);
        let tmp_byte: i8x16 = unsafe { simd_shuffle16(i8x32::from_bits(symbol_less), i8x32::splat(0),
                                                      [0, 4, 8, 12, 16, 20, 24, 28, 2, 6, 10, 14, 18, 22, 26, 30]) };
        let tmp_mask = i64x2::from_bits(tmp_byte & i8x16::new(0xf,0xf,0xf,0xf,0xf,0xf,0xf,0xf,
                                                              -0x10,-0x10,-0x10,-0x10,-0x10,-0x10,-0x10,-0x10));
        let bitmask = (tmp_mask.extract(0) | tmp_mask.extract(1)) as u64;
        let symbol_id = (64 - bitmask.leading_zeros() as u8) >> 2 ;
        self.sym_to_start_and_freq_with_div_hint(symbol_id, inv_max_and_bitlen)
    }
    #[cfg(target_arch = "x86_64")]
    #[cfg(not(feature="portable-simd"))]
    #[cfg(feature="avx2")]
    #[inline(always)]
    fn cdf_offset_to_sym_start_and_freq(&self,
                                        cdf_offset_p: Prob) -> SymStartFreq {
        let cdfmax = self.max();
        let inv_max_and_bitlen = lookup_divisor(cdfmax);
        let rescaled_cdf_offset = ((i32::from(cdf_offset_p) * i32::from(cdfmax)) >> LOG2_SCALE) as i16;
        let symbol_less = i16x16::splat(rescaled_cdf_offset).ge(self.cdf);
        let bitmask = unsafe { core::arch::x86_64::_mm256_movemask_epi8(core::arch::x86_64::__m256i::from_bits(symbol_less)) };
        let symbol_id = ((32 - (bitmask as u32).leading_zeros()) >> 1) as u8;
        self.sym_to_start_and_freq_with_div_hint(symbol_id, inv_max_and_bitlen)
    }
    #[cfg(target_arch = "x86_64")]
    #[cfg(not(feature="portable-simd"))]
    #[cfg(not(feature="avx2"))]
    #[inline(always)]
    fn cdf_offset_to_sym_start_and_freq(&self,
                                        cdf_offset_p: Prob) -> SymStartFreq {
        let cdfmax = self.max();
        let inv_max_and_bitlen = lookup_divisor(cdfmax);
        let rescaled_cdf_offset = ((i32::from(cdf_offset_p) * i32::from(cdfmax)) >> LOG2_SCALE) as i16;
        let symbol_less = i16x16::splat(rescaled_cdf_offset).ge(self.cdf);
        let tmp: i8x16 = unsafe { simd_shuffle16(i8x32::from_bits(symbol_less), i8x32::splat(0),
                                                 [0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30]) };
        let bitmask = unsafe { core::arch::x86_64::_mm_movemask_epi8(core::arch::x86_64::__m128i::from_bits(tmp)) };
        let symbol_id = (32 - (bitmask as u32).leading_zeros()) as u8;
        self.sym_to_start_and_freq_with_div_hint(symbol_id, inv_max_and_bitlen)
    }
}

#[inline(always)]
fn i16x16_to_i64x4_tuple(input: i16x16) -> (i64x4, i64x4, i64x4, i64x4) {
    let zero = i16x16::splat(0);
    unsafe {
        let widened_q0: i16x16 = simd_shuffle16(
            input, zero, [0, 16, 16, 16, 1, 16, 16, 16, 2, 16, 16, 16, 3, 16, 16, 16]);
        let widened_q1: i16x16 = simd_shuffle16(
            input, zero, [4, 16, 16, 16, 5, 16, 16, 16, 6, 16, 16, 16, 7, 16, 16, 16]);
        let widened_q2: i16x16 = simd_shuffle16(
            input, zero, [8, 16, 16, 16, 9, 16, 16, 16, 10, 16, 16, 16, 11, 16, 16, 16]);
        let widened_q3: i16x16 = simd_shuffle16(
            input, zero, [12, 16, 16, 16, 13, 16, 16, 16, 14, 16, 16, 16, 15, 16, 16, 16]);
        (i64x4::from_bits(widened_q0), i64x4::from_bits(widened_q1), i64x4::from_bits(widened_q2), i64x4::from_bits(widened_q3))
    }
}

#[inline(always)]
fn i64x4_tuple_to_i16x16(input0: i64x4, input1: i64x4, input2: i64x4, input3: i64x4) -> i16x16 {
    unsafe {
        let input01: i16x16 = simd_shuffle16(i16x16::from_bits(input0), i16x16::from_bits(input1),
                                             [0, 4, 8, 12, 16, 20, 24, 28, 0, 0, 0, 0, 0, 0, 0, 0]);
        let input23: i16x16 = simd_shuffle16(i16x16::from_bits(input2), i16x16::from_bits(input3),
                                             [0, 4, 8, 12, 16, 20, 24, 28, 0, 0, 0, 0, 0, 0, 0, 0]);
        let output: i64x4 = simd_shuffle4(i64x4::from_bits(input01), i64x4::from_bits(input23), [0, 1, 4, 5]);
        i16x16::from_bits(output)
    }
}

#[inline(always)]
fn i16x16_to_i32x8_tuple(input: i16x16) -> (i32x8, i32x8) {
    let zero = i16x16::splat(0);
    unsafe {
        let widened_lo: i16x16 = simd_shuffle16(
            input, zero, [0, 16, 1, 16, 2, 16, 3, 16, 4, 16, 5, 16, 6, 16, 7, 16]);
        let widened_hi: i16x16 = simd_shuffle16(
            input, zero, [8, 16, 9, 16, 10, 16, 11, 16, 12, 16, 13, 16, 14, 16, 15, 16]);
        (i32x8::from_bits(widened_lo), i32x8::from_bits(widened_hi))
    }
}

#[inline(always)]
fn i32x8_tuple_to_i16x16(input0: i32x8, input1: i32x8) -> i16x16 {
    unsafe {
        simd_shuffle16(i16x16::from_bits(input0), i16x16::from_bits(input1),
                       [0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30])
    }
}


impl CDF16 for SIMDFrequentistCDF16 {
    #[inline(always)]
    fn average(&self, other:&Self, mix_rate:i32) -> Self {

        let ourmax = i32::from(self.max());
        let othermax = i32::from(other.max());
        let ourmax_times_othermax = ourmax * othermax;
        let leading_zeros_combo = core::cmp::min(ourmax_times_othermax.leading_zeros(), 17);
        let desired_shift = 17 - leading_zeros_combo;

        let inv_mix_rate = (1 << BLEND_FIXED_POINT_PRECISION) - mix_rate;
        let mix_rate_v = i32x8::splat(mix_rate);
        let inv_mix_rate_v = i32x8::splat(inv_mix_rate);
        let our_max_v = i32x8::splat(ourmax);
        let other_max_v = i32x8::splat(othermax);
        let one = i32x8::splat(1);
        let (self0, self1) = i16x16_to_i32x8_tuple(self.cdf);
        let (other0, other1) = i16x16_to_i32x8_tuple(other.cdf);
        let rescaled_self0 = (self0 * other_max_v) >> desired_shift; // now we know we have at least 15 bits remaining in our space
        let rescaled_self1 = (self1 * other_max_v) >> desired_shift;
        let rescaled_other0 = (other0 * our_max_v) >> desired_shift;
        let rescaled_other1 = (other1 * our_max_v) >> desired_shift;

        let ret0 = (rescaled_self0 * mix_rate_v + rescaled_other0 * inv_mix_rate_v + one) >> (BLEND_FIXED_POINT_PRECISION as i8);
        let ret1 = (rescaled_self1 * mix_rate_v + rescaled_other1 * inv_mix_rate_v + one) >> (BLEND_FIXED_POINT_PRECISION as i8);
        SIMDFrequentistCDF16::new(i32x8_tuple_to_i16x16(ret0, ret1))
    }
    #[inline(always)]
    fn blend(&mut self, symbol: u8, speed: Speed) {
        let increment_v = i16x16::splat(speed.inc());
        let one_to_16 = i16x16::new(1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16);
        let mask_v = one_to_16.gt(i16x16::splat(i16::from(symbol)));
        self.cdf = self.cdf + (increment_v & i16x16::from_bits(mask_v));
        let mut cdf_max = self.max();
        if cdf_max >= speed.lim() {
            let cdf_bias = one_to_16;
            self.cdf = self.cdf + cdf_bias - ((self.cdf + cdf_bias) >> 2);
            cdf_max = self.max();
        }
    }
}

#[cfg(test)]
#[cfg(feature="simd")]
mod test {
    use super::{i16x16, i32x8, i64x4};
    use super::{i16x16_to_i32x8_tuple, i16x16_to_i64x4_tuple,
                i32x8_tuple_to_i16x16, i64x4_tuple_to_i16x16};
    use super::SIMDFrequentistCDF16;

    declare_common_tests!(SIMDFrequentistCDF16);

    #[test]
    fn test_cdf_simd_eq_opt() {
        use super::super::{common_tests, OptFrequentistCDF16};
        common_tests::operation_test_helper(&mut SIMDFrequentistCDF16::default(),
                                            &mut SIMDFrequentistCDF16::default(),
                                            &mut OptFrequentistCDF16::default(),
                                            &mut OptFrequentistCDF16::default());
    }

    #[test]
    fn test_i32x8_tuple_to_i16x16() {
        let input0 = i32x8::new(984414081, 278, 3410058, 421713, 3295297, 22420, 839546, 181135048);
        let input1 = i32x8::new(570597, 12477978, 124711081, 86618, 1061795, 3018810, 5691, 342342);
        let output = i32x8_tuple_to_i16x16(input0, input1);
        for i in 0..8 {
            assert_eq!(output.extract(i), (input0.extract(i) & 65535) as i16);
            assert_eq!(output.extract(i+8), (input1.extract(i) & 65535) as i16);
        }
    }
    #[test]
    fn test_i16x16_to_i32x8_tuple() {
        let input = i16x16::new(2619, 12771, 1898, 29313, 23504, 18725, 15115, 32179,
                                18593, 13755, 18706, 2073, 15715, 17696, 25568, 12775);
        let output = i16x16_to_i32x8_tuple(input);
        for i in 0..8 {
            assert_eq!(output.0.extract(i), input.extract(i) as i32);
            assert_eq!(output.1.extract(i), input.extract(i+8) as i32);
        }
    }

    #[test]
    fn test_i64x4_tuple_to_i16x16() {
        let mut seed = 1u64;
        let mut input: [i64x4; 4] = [i64x4::splat(0); 4];
        // Generate input vectors such that each 16-bit lane is unique.
        for i in 0..4 {
            let mut array: [i64; 4] = [0; 4];
            for j in 0..4 {
                array[j] = (((100 + i * 4 + j) as i64) +
                            (((200 + i * 4 + j) as i64) << 16) +
                            (((300 + i * 4 + j) as i64) << 32) +
                            (((400 + i * 4 + j) as i64) << 48));
            }
            input[i] = i64x4::load_unaligned(&array);
        }
        let output = i64x4_tuple_to_i16x16(input[0], input[1], input[2], input[3]);
        for i in 0..4 {
            assert_eq!(output.extract(i), (input[0].extract(i) & 65535) as i16);
            assert_eq!(output.extract(i+4), (input[1].extract(i) & 65535) as i16);
            assert_eq!(output.extract(i+8), (input[2].extract(i) & 65535) as i16);
            assert_eq!(output.extract(i+12), (input[3].extract(i) & 65535) as i16);
        }
    }

    #[test]
    fn test_i16x16_to_i64x4_tuple() {
        let input = i16x16::new(2619, 12771, 1898, 29313, 23504, 18725, 15115, 32179,
                                18593, 13755, 18706, 2073, 15715, 17696, 25568, 12775);
        let output = i16x16_to_i64x4_tuple(input);
        for i in 0..4 {
            assert_eq!(output.0.extract(i), input.extract(i) as i64);
            assert_eq!(output.1.extract(i), input.extract(i+4) as i64);
            assert_eq!(output.2.extract(i), input.extract(i+8) as i64);
            assert_eq!(output.3.extract(i), input.extract(i+12) as i64);
        }
    }
}
