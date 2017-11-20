fn k16bit_length(d:i16) -> u8 {
    (16 - d.leading_zeros()) as u8
}
pub const LOG_MAX_NUMERATOR: usize = 30;
pub fn compute_divisor(d: i16) -> (i64, u8) {
    let bit_len = k16bit_length(d);
    (((((( 1i64 << bit_len) - i64::from(d)) << (LOG_MAX_NUMERATOR))) / i64::from(d)) + 1, bit_len.wrapping_sub(1))
}
pub fn lookup_divisor(d: i16) -> (i64, u8) {
    compute_divisor(d)
}
pub fn fast_divide_30bit_by_16bit(num: i32, inv_denom_and_bitlen: (i64, u8)) -> i32 {
    let idiv_mul_num = i64::from(inv_denom_and_bitlen.0) * i64::from(num);
     ((idiv_mul_num >> LOG_MAX_NUMERATOR) as i32
         + (((i64::from(num) - (idiv_mul_num >> LOG_MAX_NUMERATOR)) as i32) >> 1))
      >> inv_denom_and_bitlen.1
}