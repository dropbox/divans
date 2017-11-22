use core;
use super::interface::{Prob, BaseCDF, Speed, CDF16, BLEND_FIXED_POINT_PRECISION};
use super::numeric;
use stdsimd::simd::{i16x16, i64x4, i16x8, i8x32, i8x16, u32x8, u8x16};
use stdsimd;
use stdsimd::vendor::__m256i;
#[derive(Clone,Copy)]
pub struct SIMDFrequentistCDF16 {
    pub cdf: i16x16,
    pub inv_max: (i64, u8),
}

impl SIMDFrequentistCDF16 {
    fn new(input:i16x16) -> Self {
        let mut ret = SIMDFrequentistCDF16{
            cdf:input,
            inv_max: (0, 0),
        };
        ret.inv_max = numeric::lookup_divisor(ret.max());
        ret
    }
}

impl Default for SIMDFrequentistCDF16 {
    fn default() -> Self {
        SIMDFrequentistCDF16::new(i16x16::new(4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60, 64))
    }
}


impl BaseCDF for SIMDFrequentistCDF16 {
    fn num_symbols() -> u8 { 16 }
    fn used(&self) -> bool {
        self.entropy() != Self::default().entropy()
    }
    fn max(&self) -> Prob {
        self.cdf.extract(15)
    }
    fn div_by_max(&self, val:i32) -> i32 {
        numeric::fast_divide_30bit_by_16bit(val, self.inv_max)
    }
    fn log_max(&self) -> Option<i8> { None }
    fn cdf(&self, symbol: u8) -> Prob {
        self.cdf.extract(symbol as u32)
    }
    fn valid(&self) -> bool {
        let mut prev = 0;
        let mut slice = [0i16;16];
        self.cdf.store(&mut slice, 0);
        for item in slice.iter() {
            if *item <= prev {
                return false;
            }
            prev = *item;
        }
        self.inv_max == numeric::lookup_divisor(self.max())
    }
}
fn i16x16_to_i64x4_tuple(input: i16x16) -> (i64x4,i64x4,i64x4,i64x4) {
    let upper_quad_replicated = unsafe{stdsimd::vendor::_mm256_permute4x64_epi64(i64x4::from(input), 0xee)};
    let upper_quad = unsafe{stdsimd::vendor::_mm256_castsi256_si128(__m256i::from(upper_quad_replicated))};
    let self0 = unsafe{stdsimd::vendor::_mm256_cvtepi16_epi64(i16x8::from(stdsimd::vendor::_mm256_castsi256_si128(__m256i::from(input))))};
    let self1 = unsafe{stdsimd::vendor::_mm256_cvtepi16_epi64(i16x8::from(stdsimd::vendor::_mm_alignr_epi8(stdsimd::vendor::_mm256_castsi256_si128(__m256i::from(input)),stdsimd::vendor::_mm256_castsi256_si128(__m256i::from(input)), 8)))};
    let self2 = unsafe{stdsimd::vendor::_mm256_cvtepi16_epi64(i16x8::from(upper_quad))};
    let self3 = unsafe{stdsimd::vendor::_mm256_cvtepi16_epi64(i16x8::from(stdsimd::vendor::_mm_alignr_epi8(upper_quad, upper_quad, 8)))};
    (self0, self1, self2, self3)
}
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


impl CDF16 for SIMDFrequentistCDF16 {
    fn average(&self, other:&Self, mix_rate:i32) -> Self {

        let ourmax = i64::from(self.max());
        let othermax = i64::from(other.max());
        let maxmax = core::cmp::min(ourmax, othermax);
        let lgmax = 64 - maxmax.leading_zeros();
        let inv_mix_rate = (1 << BLEND_FIXED_POINT_PRECISION) - mix_rate;
        //let cdf4567: i16x16 = std::simd::vendor::_mm256_shuffle_epi8(address, simd_shuffle8::<_, i16x16>(self.cdf, self.cdf, [4, 5, 6, 7, 4,5,6,7, 8,9,10,11,12,13,14,15]);
        let mix_rate_v = i64x4::splat(i64::from(mix_rate));
        let inv_mix_rate_v = i64x4::splat(i64::from(inv_mix_rate));
        let our_max_v = i64x4::splat(ourmax);
        let other_max_v = i64x4::splat(othermax);
        let one = i64x4::splat(1);
        let (self0, self1, self2, self3) = i16x16_to_i64x4_tuple(self.cdf);
        let (other0, other1, other2, other3) = i16x16_to_i64x4_tuple(other.cdf);
        let ret0 = (self0 * mix_rate_v * other_max_v + other0 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let ret1 = (self1 * mix_rate_v * other_max_v + other1 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let ret2 = (self2 * mix_rate_v * other_max_v + other2 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        let ret3 = (self3 * mix_rate_v * other_max_v + other3 * inv_mix_rate_v * our_max_v + one) >> (BLEND_FIXED_POINT_PRECISION + lgmax as i8);
        SIMDFrequentistCDF16::new(i64x4_tuple_to_i16x16(ret0, ret1, ret2, ret3))
        // FIXME this is missing let upper_quad = stdsimd::vendor::_mm256_extracti128_si256(__m256i::from(self.cdf), 1);
        
        //for (s, o) in retval.cdf.iter_mut().zip(other.cdf.iter()) {
        //(((i64::from(*s) * i64::from(mix_rate) * othermax + i64::from(*o) * i64::from(inv_mix_rate) * ourmax + 1) >> BLEND_FIXED_POINT_PRECISION) >> lgmax) as Prob;
        //}
    }
    #[always_inline]
    fn blend(&mut self, symbol: u8, speed: Speed) {
        let increment : i16 =
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
        let increment_v = i16x16::splat(increment);
        //let mask_v = unsafe{stdsimd::vendor::_mm256_alignr_epi8(i8x32::splat(0xff),stdsimd::vendor::_mm256_setzero_si256(), 32 - (symbol<< 1))};
        let one_to_16 = i16x16::new(1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16);
        let mask_v = unsafe{stdsimd::vendor::_mm256_cmpgt_epi16(one_to_16, i16x16::splat(i16::from(symbol)))};
        self.cdf = self.cdf + (increment_v & i16x16::from(mask_v));
        const LIMIT: i16 = 32_767 - 16 - 384 /* XXX: max possible increment */;
        let mut cdf_max = self.max();
        if cdf_max >= LIMIT {
            let cdf_bias = one_to_16;
            self.cdf = self.cdf + cdf_bias - ((self.cdf + cdf_bias) >> 2);
            cdf_max = self.max();
        }
        self.inv_max = numeric::lookup_divisor(cdf_max);
    }
}

//__mmask16 _mm256_cmpge_epi16_mask (__m256i a, __m256i b)
