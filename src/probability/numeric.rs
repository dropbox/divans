#[cfg(not(feature="division_table_gen"))]
use super::div_lut;

#[cfg(feature="simd")]
use stdsimd::simd;
#[cfg(not(feature="division_table_gen"))]
pub type DenominatorType = i16;
#[cfg(feature="division_table_gen")]
pub type DenominatorType = u16;

fn k16bit_length(d:DenominatorType) -> u8 {
    (16 - d.leading_zeros()) as u8
}
pub const LOG_MAX_NUMERATOR: usize = 31;
pub fn compute_divisor(d: DenominatorType) -> (i64, u8) {
    let bit_len = k16bit_length(d);
    (((((( 1i64 << bit_len) - i64::from(d)) << (LOG_MAX_NUMERATOR))) / i64::from(d)) + 1, bit_len.wrapping_sub(1))
}
#[cfg(not(feature="division_table_gen"))]
pub fn lookup_divisor(d: i16) -> (i64, u8) {
    div_lut::RECIPROCAL[d as u16 as usize]
}
#[inline(always)]
pub fn fast_divide_30bit_by_16bit(num: i32, inv_denom_and_bitlen: (i64, u8)) -> i32 {
    let idiv_mul_num = i64::from(inv_denom_and_bitlen.0) * i64::from(num);
     ((idiv_mul_num >> LOG_MAX_NUMERATOR) as i32
         + (((i64::from(num) - (idiv_mul_num >> LOG_MAX_NUMERATOR)) as i32) >> 1))
      >> inv_denom_and_bitlen.1
}

#[cfg(feature="simd")]
pub fn fast_divide_30bit_i64x2_by_16bit(num: simd::i64x2, inv_denom_and_bitlen: (i64, u8)) -> simd::i64x2 {
    let idiv_mul_num = simd::i64x2::splat(inv_denom_and_bitlen.0) * num;
    let idiv_mul_num_shift_max_num = idiv_mul_num >> LOG_MAX_NUMERATOR;
     (idiv_mul_num_shift_max_num
         + ((num - (idiv_mul_num_shift_max_num)) >> 1))
      >> inv_denom_and_bitlen.1
}




pub type Denominator8Type = u8;
const LOG_MAX_NUMERATOR16:usize = 15;
fn k8bit_length(d:Denominator8Type) -> u8 {
    (8 - d.leading_zeros()) as u8
}
pub fn compute_divisor8(d: Denominator8Type) -> (i32, u8) {
    let bit_len = k8bit_length(d);
    (((((( 1i32 << bit_len) - i32::from(d)) << (LOG_MAX_NUMERATOR16))) / i32::from(d)) + 1, bit_len.wrapping_sub(1))
}
#[cfg(not(feature="division_table_gen"))]
pub fn lookup_divisor8(d: u8) -> (i32, u8) {
    div_lut::RECIPROCAL8[d as u8 as usize]
}
#[inline(always)]
pub fn fast_divide_15bit_by_8bit(num: i16, inv_denom_and_bitlen: (i32, u8)) -> i16 {
    let idiv_mul_num = i32::from(inv_denom_and_bitlen.0) * i32::from(num);
     ((idiv_mul_num >> LOG_MAX_NUMERATOR16) as i16
         + (((i32::from(num) - (idiv_mul_num >> LOG_MAX_NUMERATOR16)) as i16) >> 1))
      >> inv_denom_and_bitlen.1
}

