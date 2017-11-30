mod numeric;
fn main() {
    print!("pub static RECIPROCAL8: [i32; 256] = [\n    0, ");
    for divisor in 1..256 {
        let next_str = if divisor % 16 == 15 {
           "\n    "
        } else {
           " "
        };
        let reciprocal = numeric::compute_divisor8(divisor as numeric::Denominator8Type);
        let mut fail = false;
        for num in 0u16..65535u16 {
            let correct = num as u16 /divisor;
            let trial = numeric::fast_divide_16bit_by_8bit(num as u16, reciprocal) as u16;
            if trial != correct {
                print!("FAIL: {} : {} / {} = fast: {} slow: {}\n",
                       reciprocal,
                       num,
                       divisor,
                       trial,
                       correct);
                fail = true;
            }
        }
        assert!(!fail);
        assert!(reciprocal <= (1<<30));
        print!("{},{}", reciprocal, next_str)
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
