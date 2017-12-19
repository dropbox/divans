use core;
use super::interface::{Prob, BaseCDF, Speed, CDF16, BLEND_FIXED_POINT_PRECISION, SymStartFreq, LOG2_SCALE, MAX_FREQUENTIST_PROB};
use super::numeric;
use stdsimd::simd::{i16x16, i64x4, i16x8, i8x32, i8x16, u32x8, u8x16, i64x2, i32x8};
use stdsimd;
use stdsimd::vendor::__m256i;

#[derive(Clone,Copy)]
pub struct SIMDFrequentistCDF16 {
    pub cdf: i16x16,
    pub inv_max: (i64, u8),
}

impl SIMDFrequentistCDF16 {
    #[inline(always)]
    fn new(input: i16x16) -> Self {
        let mut ret = SIMDFrequentistCDF16 {
            cdf: input,
            inv_max: (0, 0),
        };
        ret.inv_max = numeric::lookup_divisor(ret.max());
        ret
    }
}

impl Default for SIMDFrequentistCDF16 {
    #[inline(always)]
    fn default() -> Self {
        SIMDFrequentistCDF16::new(i16x16::new(4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60, 64))
    }
}

impl BaseCDF for SIMDFrequentistCDF16 {
    #[inline(always)]
    fn num_symbols() -> u8 { 16 }
    #[inline(always)]
    fn used(&self) -> bool { self.entropy() != Self::default().entropy() }
    #[inline(always)]
    fn max(&self) -> Prob { self.cdf.extract(15) }
    #[inline(always)]
    fn div_by_max(&self, val:i32) -> i32 { numeric::fast_divide_30bit_by_16bit(val, self.inv_max) }
    #[inline(always)]
    fn log_max(&self) -> Option<i8> { None }
    #[inline(always)]
    fn cdf(&self, symbol: u8) -> Prob {
        // for some reason it's way better to assert to the compiler "hey I'm within 0-15"
        self.cdf.extract(symbol as u32 & 0xf)
    }
    fn valid(&self) -> bool {
        let mut slice = [0i16; 16];
        self.cdf.store(&mut slice, 0);
        for it in slice[0..15].iter().zip(slice[1..16].iter()) {
            let (prev, next) = it;
            if (*next <= *prev) {
                return false;
            }
        }
        self.inv_max == numeric::lookup_divisor(self.max())
    }
    /* //slower
    fn sym_to_start_and_freq(&self,
                             sym: u8) -> SymStartFreq {
        let prev_cur = i64x2::new(if sym != 0 {self.cdf(sym - 1) as u64 as i64} else {0},
                                  self.cdf(sym) as u64 as i64);
        let scaled_prev_cur = prev_cur << LOG2_SCALE;
        let prev_cur_over_max = numeric::fast_divide_30bit_i64x2_by_16bit(scaled_prev_cur, self.inv_max);
        let cdf_prev = prev_cur_over_max.extract(0);
        let freq = prev_cur_over_max.extract(1) - cdf_prev;
        SymStartFreq {
            start: cdf_prev as Prob + 1, // major hax
            freq:  freq as Prob - 1, // don't want rounding errors to work out unfavorably
            sym: sym,
        }
}*/
    #[cfg(feature="avx2")]
    #[inline(always)]
    fn cdf_offset_to_sym_start_and_freq(&self,
                                        cdf_offset_p: Prob) -> SymStartFreq {
        let rescaled_cdf_offset = ((i32::from(cdf_offset_p) * i32::from(self.max())) >> LOG2_SCALE) as i16;
        let symbol_less = unsafe{stdsimd::vendor::_mm256_cmpgt_epi16(
            i16x16::splat(rescaled_cdf_offset),
            self.cdf - i16x16::splat(1))};
        let bitmask = unsafe{stdsimd::vendor::_mm256_movemask_epi8(i8x32::from(symbol_less))};
        let symbol_id = ((32 - (bitmask as u32).leading_zeros()) >> 1) as u8;
        self.sym_to_start_and_freq(symbol_id)
    }
    #[cfg(not(feature="avx2"))]
    #[inline(always)]
    fn cdf_offset_to_sym_start_and_freq(&self,
                                        cdf_offset_p: Prob) -> SymStartFreq {
        let rescaled_cdf_offset = ((i32::from(cdf_offset_p) * i32::from(self.max())) >> LOG2_SCALE) as i16;
        let symbol_less = unsafe{stdsimd::vendor::_mm256_cmpgt_epi16(
            i16x16::splat(rescaled_cdf_offset),
            self.cdf - i16x16::splat(1))};
        let lower_bitmask = unsafe{stdsimd::vendor::_mm_movemask_epi8(i8x16::from(stdsimd::vendor::_mm256_castsi256_si128(__m256i::from(symbol_less))))} as u32;
        let upper_quad_cmp = unsafe{stdsimd::vendor::_mm256_permute4x64_epi64(i64x4::from(symbol_less),
                                                                              0xee)};

        let upper_bitmask = unsafe{stdsimd::vendor::_mm_movemask_epi8(i8x16::from(stdsimd::vendor::_mm256_castsi256_si128(__m256i::from(upper_quad_cmp))))} as u32;
        let bitmask = (upper_bitmask << 16) | lower_bitmask;
        let symbol_id = ((32 - (bitmask as u32).leading_zeros()) >> 1) as u8;
        self.sym_to_start_and_freq(symbol_id)
    }
}
#[inline(always)]
fn i16x16_to_i64x4_tuple(input: i16x16) -> (i64x4,i64x4,i64x4,i64x4) {
    let upper_quad_replicated = unsafe{stdsimd::vendor::_mm256_permute4x64_epi64(i64x4::from(input), 0xee)};
    let upper_quad = unsafe{stdsimd::vendor::_mm256_castsi256_si128(__m256i::from(upper_quad_replicated))};
    let self0 = unsafe{stdsimd::vendor::_mm256_cvtepi16_epi64(i16x8::from(stdsimd::vendor::_mm256_castsi256_si128(__m256i::from(input))))};
    let self1 = unsafe{stdsimd::vendor::_mm256_cvtepi16_epi64(i16x8::from(stdsimd::vendor::_mm_alignr_epi8(stdsimd::vendor::_mm256_castsi256_si128(__m256i::from(input)),stdsimd::vendor::_mm256_castsi256_si128(__m256i::from(input)), 8)))};
    let self2 = unsafe{stdsimd::vendor::_mm256_cvtepi16_epi64(i16x8::from(upper_quad))};
    let self3 = unsafe{stdsimd::vendor::_mm256_cvtepi16_epi64(i16x8::from(stdsimd::vendor::_mm_alignr_epi8(upper_quad, upper_quad, 8)))};
    (self0, self1, self2, self3)
}
#[inline(always)]
fn i64x4_tuple_to_i16x16(input0: i64x4, input1: i64x4, input2: i64x4, input3: i64x4) -> i16x16 {
    //FIXME: can potentially do this as some shuffles ??
    i16x16::new(input0.extract(0) as i16,
                input0.extract(1) as i16,
                input0.extract(2) as i16,
                input0.extract(3) as i16,
                input1.extract(0) as i16,
                input1.extract(1) as i16,
                input1.extract(2) as i16,
                input1.extract(3) as i16,
                input2.extract(0) as i16,
                input2.extract(1) as i16,
                input2.extract(2) as i16,
                input2.extract(3) as i16,
                input3.extract(0) as i16,
                input3.extract(1) as i16,
                input3.extract(2) as i16,
                input3.extract(3) as i16)
}

extern "platform-intrinsic" {
    pub fn simd_shuffle16<T, U>(x: T, y: T, idx: [u32; 16]) -> U;
}

#[inline(always)]
fn i16x16_to_i32x8_tuple(input: i16x16) -> (i32x8, i32x8) {
    let zero = i16x16::splat(0);
    unsafe {
        let widened_lo: i16x16 = simd_shuffle16(
            input, zero, [0, 16, 1, 16, 2, 16, 3, 16, 4, 16, 5, 16, 6, 16, 7, 16]);
        let widened_hi: i16x16 = simd_shuffle16(
            input, zero, [8, 16, 9, 16, 10, 16, 11, 16, 12, 16, 13, 16, 14, 16, 15, 16]);
        (i32x8::from(widened_lo), i32x8::from(widened_hi))
    }
}

#[inline(always)]
fn i32x8_tuple_to_i16x16(input0: i32x8, input1: i32x8) -> i16x16 {
    unsafe {
        simd_shuffle16(i16x16::from(input0), i16x16::from(input1),
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
        let increment_v = i16x16::splat(speed as i16);
        let one_to_16 = i16x16::new(1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16);
        let mask_v = unsafe {
            stdsimd::vendor::_mm256_cmpgt_epi16(one_to_16, i16x16::splat(i16::from(symbol)))
        };
        self.cdf = self.cdf + (increment_v & i16x16::from(mask_v));
        let mut cdf_max = self.max();
        if cdf_max >= MAX_FREQUENTIST_PROB {
            let cdf_bias = one_to_16;
            self.cdf = self.cdf + cdf_bias - ((self.cdf + cdf_bias) >> 2);
            cdf_max = self.max();
        }
        self.inv_max = numeric::lookup_divisor(cdf_max);
    }
}

//__mmask16 _mm256_cmpge_epi16_mask (__m256i a, __m256i b)

#[cfg(test)]
mod test {
    use super::{i16x16, i32x8, i32x8_tuple_to_i16x16, i16x16_to_i32x8_tuple};

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
}
