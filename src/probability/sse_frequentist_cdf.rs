
use core;
use super::interface::{Prob, BaseCDF, Speed, CDF16, BLEND_FIXED_POINT_PRECISION, SymStartFreq, LOG2_SCALE};
use super::numeric;
use stdsimd::simd::{i16x16, i64x4, i16x8, i8x32, i8x16, u32x8, u8x16, i64x2};
use stdsimd;
use stdsimd::vendor::__m256i;
use stdsimd::vendor::__m128i;
#[derive(Clone,Copy)]
pub struct SSEFrequentistCDF16 {
    pub cdf0: i16x8,
    pub cdf1: i16x8,
    pub inv_max: (i64, u8),
}

impl SSEFrequentistCDF16 {
    fn new(input0:i16x8, input1:i16x8) -> Self {
        let mut ret = SSEFrequentistCDF16{
            cdf0:input0,
            cdf1:input1,
            inv_max: (0, 0),
        };
        ret.inv_max = numeric::lookup_divisor(ret.max());
        ret
    }
}

impl Default for SSEFrequentistCDF16 {
    fn default() -> Self {
        SSEFrequentistCDF16::new(i16x8::new(4, 8, 12, 16, 20, 24, 28, 32), i16x8::new(36, 40, 44, 48, 52, 56, 60, 64))
    }
}


impl BaseCDF for SSEFrequentistCDF16 {
    fn num_symbols() -> u8 { 16 }
    fn used(&self) -> bool {
        self.entropy() != Self::default().entropy()
    }
    fn max(&self) -> Prob {
        self.cdf1.extract(7)
    }
    fn div_by_max(&self, val:i32) -> i32 {
        numeric::fast_divide_30bit_by_16bit(val, self.inv_max)
    }
    fn log_max(&self) -> Option<i8> { None }
    fn cdf(&self, symbol: u8) -> Prob {
        (self.cdf0.extract((symbol as u32) & 7)& !-(((symbol>>3) as i16)&1)) |
            self.cdf1.extract((symbol as u32) & 7)& -(((symbol as i16)>>3)&1i16)
    }
    fn valid(&self) -> bool {
        let mut prev = 0;
        let mut slice0 = [0i16;8];
        let mut slice1 = [0i16;8];
        self.cdf0.store(&mut slice0, 0);
        self.cdf1.store(&mut slice1, 0);
        for item in slice0.iter().chain(slice1.iter()) {
            if *item <= prev {
                return false;
            }
            prev = *item;
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
    /*
    #[cfg(feature="avx2")]
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
    }*/
}
fn i16x8_tuple_to_i64x2_tuple(input0: i16x8, input1:i16x8) -> (i64x2,i64x2,i64x2,i64x2,i64x2,i64x2,i64x2,i64x2) {
    let upper_quad = input1;
    let self0 = i64x2::new(input0.extract(0) as u16 as i64,
                           input0.extract(1) as u16 as i64);
    let self1 = i64x2::new(input0.extract(2) as u16 as i64,
                           input0.extract(3) as u16 as i64);
    let self2 = i64x2::new(input0.extract(4) as u16 as i64,
                           input0.extract(5) as u16 as i64);
    let self3 = i64x2::new(input0.extract(6) as u16 as i64,
                           input0.extract(7) as u16 as i64);
    let self4 = i64x2::new(input1.extract(0) as u16 as i64,
                           input1.extract(1) as u16 as i64);
    let self5 = i64x2::new(input1.extract(2) as u16 as i64,
                           input1.extract(3) as u16 as i64);
    let self6 = i64x2::new(input1.extract(4) as u16 as i64,
                           input1.extract(5) as u16 as i64);
    let self7 = i64x2::new(input1.extract(6) as u16 as i64,
                           input1.extract(7) as u16 as i64);
    (self0, self1, self2, self3, self4, self5, self6, self7)
}
fn i64x2_tuple_to_i16x8_tuple(input0: i64x2, input1: i64x2, input2: i64x2, input3: i64x2,
                              input4: i64x2, input5: i64x2, input6: i64x2, input7: i64x2) -> (i16x8, i16x8) {
    //FIXME: can potentially do this as some shuffles ??
    (i16x8::new(input0.extract(0) as i16,
                input0.extract(1) as i16,
                input1.extract(0) as i16,
                input1.extract(1) as i16,
                input2.extract(0) as i16,
                input2.extract(1) as i16,
                input3.extract(0) as i16,
                input3.extract(1) as i16),
     i16x8::new(input4.extract(0) as i16,
                input4.extract(1) as i16,
                input5.extract(0) as i16,
                input5.extract(1) as i16,
                input6.extract(0) as i16,
                input6.extract(1) as i16,
                input7.extract(0) as i16,
                input7.extract(1) as i16))
}


impl CDF16 for SSEFrequentistCDF16 {
    fn average(&self, other:&Self, mix_rate:i32) -> Self {

        let ourmax = i64::from(self.max());
        let othermax = i64::from(other.max());
        let maxmax = core::cmp::min(ourmax, othermax);
        let lgmax = 64 - maxmax.leading_zeros();
        let inv_mix_rate = (1 << BLEND_FIXED_POINT_PRECISION) - mix_rate;
        //let cdf4567: i16x16 = std::simd::vendor::_mm256_shuffle_epi8(address, simd_shuffle8::<_, i16x16>(self.cdf, self.cdf, [4, 5, 6, 7, 4,5,6,7, 8,9,10,11,12,13,14,15]);
        let mix_rate_v = i64x2::splat(i64::from(mix_rate));
        let inv_mix_rate_v = i64x2::splat(i64::from(inv_mix_rate));
        let our_max_v = i64x2::splat(ourmax);
        let other_max_v = i64x2::splat(othermax);
        let one = i64x2::splat(1);
        let (self0, self1, self2, self3, self4, self5, self6, self7) = i16x8_tuple_to_i64x2_tuple(self.cdf0, self.cdf1);
        let (other0, other1, other2, other3, other4, other5, other6, other7) = i16x8_tuple_to_i64x2_tuple(other.cdf0, other.cdf1);
        let ret0 = (self0 * mix_rate_v * other_max_v + other0 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let ret1 = (self1 * mix_rate_v * other_max_v + other1 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let ret2 = (self2 * mix_rate_v * other_max_v + other2 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let ret3 = (self3 * mix_rate_v * other_max_v + other3 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let ret4 = (self4 * mix_rate_v * other_max_v + other4 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let ret5 = (self5 * mix_rate_v * other_max_v + other5 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let ret6 = (self6 * mix_rate_v * other_max_v + other6 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let ret7 = (self7 * mix_rate_v * other_max_v + other7 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let args = i64x2_tuple_to_i16x8_tuple(ret0, ret1, ret2, ret3, ret4, ret5, ret6, ret7);
        SSEFrequentistCDF16::new(args.0, args.1)
        // FIXME this is missing let upper_quad = stdsimd::vendor::_mm256_extracti128_si256(__m256i::from(self.cdf), 1);
        
        //for (s, o) in retval.cdf.iter_mut().zip(other.cdf.iter()) {
        //(((i64::from(*s) * i64::from(mix_rate) * othermax + i64::from(*o) * i64::from(inv_mix_rate) * ourmax + 1) >> BLEND_FIXED_POINT_PRECISION) >> lgmax) as Prob;
        //}
    }
    #[inline(always)]
    fn blend(&mut self, symbol: u8, speed: Speed) {
        let increment : i16 = speed as i16;
        let increment_v = i16x8::splat(increment);
        //let mask_v = unsafe{stdsimd::vendor::_mm256_alignr_epi8(i8x32::splat(0xff),stdsimd::vendor::_mm256_setzero_si256(), 32 - (symbol<< 1))};
        let one_to_8 = i16x8::new(1,2,3,4,5,6,7, 8);
        let nine_to_16 = i16x8::new(9,10,11,12,13,14,15,16);
        let mask_v0 = unsafe{stdsimd::vendor::_mm_cmpgt_epi16(one_to_8, i16x8::splat(i16::from(symbol)))};
        let mask_v1 = unsafe{stdsimd::vendor::_mm_cmpgt_epi16(nine_to_16, i16x8::splat(i16::from(symbol)))};
        let mut cache=[0i16;16];
        for i in 0..16{
            cache[i] = self.cdf(i as u8);
        }
        self.cdf0 = self.cdf0 + (increment_v & i16x8::from(mask_v0));
        self.cdf1 = self.cdf1 + (increment_v & i16x8::from(mask_v1));
        const LIMIT: i16 = 32_767 - 16 - 384 /* XXX: max possible increment */;
        let mut cdf_max = self.max();
        if cdf_max >= LIMIT {
            let cdf_bias0 = one_to_8;
            let cdf_bias1 = nine_to_16;
            self.cdf0 = self.cdf0 + cdf_bias0 - ((self.cdf0 + cdf_bias0) >> 2);
            self.cdf1 = self.cdf1 + cdf_bias1 - ((self.cdf1 + cdf_bias1) >> 2);
            cdf_max = self.max();
        }
        self.inv_max = numeric::lookup_divisor(cdf_max);
    }
}

//__mmask16 _mm256_cmpge_epi16_mask (__m256i a, __m256i b)

