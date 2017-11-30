mod numeric;
fn main() {
    print!("pub static RECIPROCAL8: [(i32, u8); 256] = [\n    (0,0), ");
    for divisor in 1..256 {
        let next_str = if divisor % 16 == 15 {
           "\n    "
        } else {
           " "
        };
        let reciprocal = numeric::compute_divisor8(divisor as numeric::Denominator8Type);
        for num in 0u16..32768u16 {
            assert_eq!(num as i16 /divisor, numeric::fast_divide_15bit_by_8bit(num as i16, reciprocal));
        }
        print!("({},{}),{}", reciprocal.0, numeric::compute_divisor8(divisor as numeric::Denominator8Type).1, next_str)
    }
    print!("];\n");
    print!("pub static RECIPROCAL: [(i64, u8); 65536] = [\n    (0,0), ");
    for divisor in 1..65536 {
        let next_str = if divisor % 16 == 15 {
           "\n    "
        } else {
           " "
        };
        let reciprocal = numeric::compute_divisor(divisor as numeric::DenominatorType);
        for num in 0..65536 {
            assert_eq!((num<<15)/divisor, numeric::fast_divide_30bit_by_16bit(num << 15, reciprocal));
        }
        print!("({},{}),{}", reciprocal.0, numeric::compute_divisor(divisor as numeric::DenominatorType).1, next_str)
    }
    print!("];\n");
}
