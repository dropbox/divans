use super::{BLEND_FIXED_POINT_PRECISION, CDF16, LOG2_SCALE, Prob, ProbRange, Speed};

#[cfg(test)]
pub fn test_sym_to_start_and_freq<T: CDF16>() {
    let mut cdf = T::default();
    for i in 0..100 {
        cdf.blend((i & 0xf) as u8, Speed::MED);
        let mut last_prob_range: ProbRange = ProbRange { start:0, freq:0 };
        for sym in 0..16 {
            let result = cdf.sym_to_start_and_freq(sym as u8);
            assert_eq!(sym as u8, result.sym);
            // NOTE: the +1 is to mirror the default implementation of sym_to_start_and_freq,
            // which does +1 to the interpolated Prob value.
            let expected_start: Prob = 1 + if sym == 0 { 0 } else {
                last_prob_range.start + last_prob_range.freq
            };
            assert_eq!(result.range.start, expected_start);
            last_prob_range = result.range.clone();
        }
    }
}

#[cfg(test)]
pub fn test_cdf_offset_to_sym_start_and_freq<T: CDF16>() {
    let mut cdf = T::default();
    for i in 0..100 {
        cdf.blend((i & 0xf) as u8, Speed::MED);
        let mut prev_sym: u8 = 0;
        for val in 0..(1i32 << LOG2_SCALE) {
            let result = cdf.cdf_offset_to_sym_start_and_freq(val as Prob);
            // TODO: The following comparisons should not have +1's, but
            // cdf_offset_to_sym_start_and_freq(...) implementation at the moment is HAX.
            assert!(prev_sym <= result.sym);
            // check that val falls in the range defined by the return value.
            assert!(result.range.start as i32 <= val + 1);
            assert!(val <= (result.range.start as i32) + (result.range.freq as i32));
            prev_sym = result.sym;
        }
        assert_eq!(prev_sym, 15);
    }
}

#[allow(unused)]
fn simple_rand(state: &mut u64) -> u32 {
    const RAND_MAX : u32 = 32_767;
    *state = (*state).wrapping_mul(1_103_515_245).wrapping_add(12_345);
    ((*state / 65_536) as u32 % (RAND_MAX + 1)) as u32
}

#[cfg(test)]
pub fn test_stationary_probability<T: CDF16>() {
    let mut cdf = T::default();
    let groundtruth_pdf: [(u32, u32); 16] = [(0,1), (0,1), (1,16), (0,1),
                                        (1,32), (1,32), (0,1), (0,1),
                                        (1,8), (0,1), (0,1), (0,1),
                                        (1,5), (1,5), (1,5), (3,20)];

    // compute CDF manually
    const CDF_MAX : u32 = 32_767;
    let mut cutoffs: [u32; 16] = [0; 16];
    let mut sum_prob: f32 = 0.0f32;
    for i in 0..16 {
        sum_prob += (groundtruth_pdf[i].0 as f32) / (groundtruth_pdf[i].1 as f32);
        cutoffs[i] = (((CDF_MAX + 1) as f32) * sum_prob).round() as u32;
    }
    assert_eq!(cutoffs[15], CDF_MAX + 1);

    // make sure we have all probability taken care of
    let mut seed = 1u64;
    let num_trials = 1000000usize;
    for i in 0..num_trials {
        let rand_num = simple_rand(&mut seed) as u32;
        for j in 0..16 {
            if rand_num < cutoffs[j] {
                // we got an j as the next symbol
                cdf.blend(j as u8, Speed::MED);
                assert!(cdf.valid());
                break;
            }
            assert!(j != 15); // should have broken
        }
    }
    for i in 0..16 {
        let actual = (cdf.pdf(i as u8) as f32) / (cdf.max() as f32);
        let expected = (groundtruth_pdf[i].0 as f32) / (groundtruth_pdf[i].1 as f32);
        let abs_delta = (expected - actual).abs();
        let rel_delta = abs_delta / expected;  // may be nan
        // TODO: These bounds should be tightened.
        assert!(rel_delta < 0.15f32 || abs_delta < 0.014f32);
    }
}

#[cfg(test)]
pub fn test_nonzero_pdf<T: CDF16>() {
    // This is a regression test
    let mut cdf = T::default();
    for _ in 0..1000000 {
        cdf.blend(15, Speed::MED);
    }
    for i in 0..15 {
        assert!(cdf.pdf(i) > 0);
    }
}

macro_rules! define_common_tests_helper {
    ($cdf_ty: ident; $($test_name: ident),+) => {
        $(
            #[test]
            fn $test_name() {
                use super::super::common_tests;
                common_tests::$test_name::<$cdf_ty>();
            }
        )+
    };
}

#[macro_export]
macro_rules! declare_common_tests {
    ($cdf_ty: ident) => {
        define_common_tests_helper!($cdf_ty;
                                    test_sym_to_start_and_freq,
                                    test_cdf_offset_to_sym_start_and_freq,
                                    test_stationary_probability,
                                    test_nonzero_pdf);
    }
}

pub fn assert_cdf_eq<CDF16A: CDF16, CDF16B: CDF16>(cdf0: &CDF16A, cdf1: &CDF16B) {
    assert_eq!(cdf0.max(), cdf1.max());
    for sym in 0..16 {
        assert_eq!(cdf0.cdf(sym as u8), cdf1.cdf(sym as u8));
    }
    assert!(cdf0.valid());
    assert!(cdf1.valid());
}

pub fn assert_cdf_similar<CDF16A: CDF16, CDF16B: CDF16>(cdf0: &CDF16A, cdf1: &CDF16B) {
    let max0 = cdf0.max() as i64;
    let max1 = cdf1.max() as i64;
    for sym in 0..16 {
        let sym0cdf = i64::from(cdf0.cdf(sym as u8));
        let sym1cdf = i64::from(cdf1.cdf(sym as u8));
        let cmp0 = sym0cdf * max1;
        let cmp1 = sym1cdf * max0;
        let delta = if cmp0 < cmp1 { cmp1.wrapping_sub(cmp0) } else { cmp0.wrapping_sub(cmp1) };
        assert!(delta < max1 * max0 / 160);
    }
    assert!(cdf0.valid());
    assert!(cdf1.valid());
}

pub fn operation_test_helper<CDFA: CDF16, CDFB: CDF16> (cdf0a: &mut CDFA, cdf1a: &mut CDFA, cdf0b: &mut CDFB, cdf1b: &mut CDFB) {
    assert_cdf_eq(cdf0a, cdf0b);
    assert_cdf_eq(cdf1a, cdf1b);
    let symbol_buffer0 = [0u8, 0u8, 0u8, 0u8, 0u8, 1u8, 2u8, 3u8, 4u8, 5u8, 5u8, 5u8, 5u8, 5u8, 5u8,
                          6u8, 7u8, 8u8, 8u8, 9u8, 9u8, 10u8, 10u8, 10u8, 10u8, 10u8, 10u8, 10u8,
                          10u8, 10u8, 10u8, 11u8, 12u8, 12u8, 12u8, 13u8, 13u8, 13u8, 14u8, 15u8,
                          15u8, 15u8, 15u8, 15u8, 15u8, 15u8];
    let symbol_buffer1 = [0u8, 0u8, 0u8, 0u8, 0u8, 1u8, 2u8, 3u8, 4u8, 5u8, 5u8, 5u8, 5u8, 5u8, 5u8];
    for sym in symbol_buffer0.iter() {
        cdf0a.blend(*sym, Speed::MED);
        cdf0b.blend(*sym, Speed::MED);
        assert_cdf_eq(cdf0a, cdf0b);
    }
    assert_cdf_similar(&cdf0a.average(cdf1a, (1<<BLEND_FIXED_POINT_PRECISION)>>2), &cdf0b.average(cdf1b, (1<<BLEND_FIXED_POINT_PRECISION)>>2));
    for sym in symbol_buffer1.iter() {
        cdf0a.blend(*sym, Speed::MED);
        cdf0b.blend(*sym, Speed::MED);
        assert_cdf_eq(cdf0a, cdf0b);
    }
    let all = (1<<BLEND_FIXED_POINT_PRECISION);
    let half = (1<<BLEND_FIXED_POINT_PRECISION)>>1;
    let quarter = (1<<BLEND_FIXED_POINT_PRECISION)>>2;
    let threequarters = half + quarter;;

    assert_cdf_eq(&cdf0a.average(cdf1a, quarter), &cdf0b.average(cdf1b, quarter));
    assert_cdf_eq(&cdf0a.average(cdf1a, half), &cdf0b.average(cdf1b, half));
    assert_cdf_eq(&cdf0a.average(cdf1a, threequarters), &cdf0b.average(cdf1b, threequarters));
    assert_cdf_eq(&cdf0a.average(cdf1a, 0), &cdf0b.average(cdf1b, 0));
    assert_cdf_eq(&cdf0a.average(cdf1a, all), &cdf0b.average(cdf1b, all));
    assert_cdf_similar(&cdf0a.average(cdf1a, 0), cdf1a);
    assert_cdf_similar(&cdf0a.average(cdf1a, all), cdf0a);
    assert_cdf_similar(&cdf0b.average(cdf1b, 0), cdf1b);
    assert_cdf_similar(&cdf0b.average(cdf1b, all), cdf0b);
}
