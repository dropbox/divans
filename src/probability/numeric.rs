#[cfg(not(feature="division_table_gen"))]
use super::div_lut;

#[cfg(feature="simd")]
use core::simd;
#[cfg(not(feature="division_table_gen"))]
pub type DenominatorType = i16;
#[cfg(feature="division_table_gen")]
pub type DenominatorType = u16;
#[inline(always)]
fn k16bit_length(d:DenominatorType) -> u8 {
    (16 - d.leading_zeros()) as u8
}
pub const LOG_MAX_NUMERATOR: usize = 31;
#[inline(always)]
pub fn compute_divisor(d: DenominatorType) -> (i64, u8) {
    let bit_len = k16bit_length(d);
    (((((( 1i64 << bit_len) - i64::from(d)) << (LOG_MAX_NUMERATOR))) / i64::from(d)) + 1, bit_len.wrapping_sub(1))
}
#[cfg(not(feature="division_table_gen"))]
#[inline(always)]
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
#[inline(always)]
pub fn fast_divide_30bit_i64x2_by_16bit(num: simd::i64x2, inv_denom_and_bitlen: (i64, u8)) -> simd::i64x2 {
    let idiv_mul_num = simd::i64x2::splat(inv_denom_and_bitlen.0) * num;
    let idiv_mul_num_shift_max_num = idiv_mul_num >> LOG_MAX_NUMERATOR;
     (idiv_mul_num_shift_max_num
         + ((num - (idiv_mul_num_shift_max_num)) >> 1))
      >> inv_denom_and_bitlen.1
}




pub type Denominator8Type = u8;
const SHIFT_16_BY_8:usize = 24;

#[inline(always)]
pub fn compute_divisor8(d: Denominator8Type) -> i32 {
    let del = 1;
    del +  (1 << SHIFT_16_BY_8) / i32::from(d)
}
#[cfg(not(feature="division_table_gen"))]
#[inline(always)]
pub fn lookup_divisor8(d: u8) -> i32 {
    div_lut::RECIPROCAL8[d as u8 as usize]
}
#[inline(always)]
pub fn fast_divide_16bit_by_8bit(num: u16, inv_denom_and_bitlen: i32) -> i16 {
    (i64::from(inv_denom_and_bitlen) * i64::from(num) >> SHIFT_16_BY_8) as i16
}


#[cfg(test)]
mod test {
    use super::{fast_divide_30bit_by_16bit, lookup_divisor};

    fn divide_30bit_by_16bit(num: i32, denom: i16) -> i32 {
        fast_divide_30bit_by_16bit(num, lookup_divisor(denom))
    }

    #[test]
    fn test_divide() {
        let nums: [i32; 10] = [3032127, 5049117, 16427165, 23282359, 35903174,
                               132971515, 163159927, 343856773, 935221996, 1829347323];
        let denoms: [i16; 10] = [115, 248, 267, 764, 1337, 4005, 4965, 9846, 24693, 31604];
        for n in nums.into_iter() {
            for d in denoms.into_iter() {
                let reference = n / (*d as i32);
                let actual = divide_30bit_by_16bit(*n, *d);
                assert_eq!(reference, actual);
            }
        }
    }
}
