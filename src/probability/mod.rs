// Copyright 2017 Dropbox, Inc
//
//   Licensed under the Apache License, Version 2.0 (the "License");
//   you may not use this file except in compliance with the License.
//   You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
//   Unless required by applicable law or agreed to in writing, software
//   distributed under the License is distributed on an "AS IS" BASIS,
//   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//   See the License for the specific language governing permissions and
//   limitations under the License.

#![allow(unused)]
use core;
use core::clone::Clone;
pub mod interface;
pub mod external_cdf;
pub mod blend_cdf;
pub mod frequentist_cdf;
pub mod div_lut;
pub mod numeric;
#[cfg(feature="simd")]
pub mod simd_frequentist_cdf;
pub mod opt_frequentist_cdf;
pub use self::interface::{BaseCDF, CDF16, CDF2, Speed, Prob, LOG2_SCALE, BLEND_FIXED_POINT_PRECISION};
pub use self::blend_cdf::{BlendCDF16};
pub use self::frequentist_cdf::FrequentistCDF16;
pub use self::external_cdf::ExternalProbCDF16;
#[cfg(feature="simd")]
pub use self::simd_frequentist_cdf::SIMDFrequentistCDF16;
pub use self::opt_frequentist_cdf::OptFrequentistCDF16;
mod test {
    use super::{BaseCDF, CDF16, BLEND_FIXED_POINT_PRECISION};
    use super::blend_cdf::{BlendCDF16, to_blend, to_blend_lut};
    use super::frequentist_cdf::FrequentistCDF16;
    use super::opt_frequentist_cdf::OptFrequentistCDF16;
        #[cfg(feature="simd")]
    use super::simd_frequentist_cdf::SIMDFrequentistCDF16;
    use super::Speed; //super speed!
    #[test]
    fn test_blend_lut() {
        for i in 0..16 {
            let a = to_blend(i as u8);
            let b = to_blend_lut(i as u8);
            for j in 0..16 {
                assert_eq!(a[j], b[j]);
            }
        }
    }
    fn assert_cdf_eq<CDFA: CDF16, CDFB: CDF16>(cdf0: &CDFA, cdf1: &CDFB) {
       assert_eq!(cdf0.max(), cdf1.max());
       for sym in 0..16 {
          assert_eq!(cdf0.cdf(sym as u8), cdf1.cdf(sym as u8));
       }
       assert!(cdf0.valid());
       assert!(cdf1.valid());
    }
    fn operation_test_helper<CDFA: CDF16, CDFB: CDF16> (cdf0a: &mut CDFA, cdf1a: &mut CDFA, cdf0b: &mut CDFB, cdf1b: &mut CDFB) {
        assert_cdf_eq(cdf0a, cdf0b);
        assert_cdf_eq(cdf1a, cdf1b);
        let symbol_buffer0 = [0u8, 0u8, 0u8, 0u8, 0u8, 1u8, 2u8, 3u8, 4u8, 5u8, 5u8, 5u8, 5u8, 5u8, 5u8, 6u8, 7u8, 8u8, 8u8,
                             9u8, 9u8, 10u8, 10u8, 10u8, 10u8, 10u8, 10u8, 10u8, 10u8, 10u8, 10u8, 11u8, 12u8, 12u8, 12u8,
                             13u8, 13u8, 13u8, 14u8, 15u8, 15u8, 15u8, 15u8, 15u8, 15u8, 15u8];
        let symbol_buffer1 = [0u8, 0u8, 0u8, 0u8, 0u8, 1u8, 2u8, 3u8, 4u8, 5u8, 5u8, 5u8, 5u8, 5u8, 5u8];
        for sym in symbol_buffer0.iter() {
              cdf0a.blend(*sym, Speed::MED);
              cdf0b.blend(*sym, Speed::MED);
              assert_cdf_eq(cdf0a, cdf0b);
        }
        assert_cdf_eq(&cdf0a.average(cdf1a, (1<<BLEND_FIXED_POINT_PRECISION)>>2), &cdf0b.average(cdf1b, (1<<BLEND_FIXED_POINT_PRECISION)>>2));
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
    }
    #[cfg(feature="simd")]
    #[test]
    fn test_cdf_simd_eq_opt() {
       operation_test_helper(&mut SIMDFrequentistCDF16::default(),
       &mut SIMDFrequentistCDF16::default(),
       &mut OptFrequentistCDF16::default(),
       &mut OptFrequentistCDF16::default());
    }
    #[test]
    fn test_cdf_opt_eq_baseline() {
       operation_test_helper(&mut FrequentistCDF16::default(),&mut FrequentistCDF16::default(),
                             &mut OptFrequentistCDF16::default(), &mut OptFrequentistCDF16::default());
    }
    #[allow(unused)]
    const RAND_MAX : u32 = 32_767;
    #[allow(unused)]
    fn simple_rand(state: &mut u64) -> u32 {
        *state = (*state).wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((*state / 65_536) as u32 % (RAND_MAX + 1)) as u32
    }

    #[allow(unused)]
    #[cfg(test)]
    fn test_random_cdf<C: CDF16>(mut prob_state: C,
                                 rand_table : [(u32, u32); 16],
                                 num_trials: usize) -> C {
        let mut cutoffs : [u32; 16] = [0; 16];
        let mut sum_prob : f32 = 0.0f32;
        for i in 0..16 {
            sum_prob += (rand_table[i].0 as f32) / (rand_table[i].1 as f32);
            cutoffs[i] = (((RAND_MAX + 1) as f32) * sum_prob).round() as u32;
        }
        assert_eq!(cutoffs[15], RAND_MAX + 1);
        // make sure we have all probability taken care of
        let mut seed = 1u64;
        for i in 0..num_trials {
            let rand_num = simple_rand(&mut seed) as u32;
            for j in 0..16 {
                if rand_num < cutoffs[j] {
                    // we got an j as the next symbol
                    prob_state.blend(j as u8, Speed::MED);
                    assert!(prob_state.valid());
                    break;
                }
                assert!(j != 15); // should have broken
            }
        }
        for i in 0..16 {
            let actual = (prob_state.pdf(i as u8) as f32) / (prob_state.max() as f32);
            let expected = (rand_table[i].0 as f32) / (rand_table[i].1 as f32);
            let abs_delta = (expected - actual).abs();
            let rel_delta = abs_delta / expected;  // may be nan
            // TODO: These bounds should be tightened.
            assert!(rel_delta < 0.15f32 || abs_delta < 0.014f32);
        }
        prob_state
    }
    #[test]
    fn test_stationary_probability_blend_cdf() {
        let rm = RAND_MAX as u32;
        test_random_cdf(BlendCDF16::default(),
                        [(0,1), (0,1), (1,16), (0,1),
                         (1,32), (1,32), (0,1), (0,1),
                         (1,8), (0,1), (0,1), (0,1),
                         (1,5), (1,5), (1,5), (3,20)],
                        1000000);
    }
    #[test]
    fn test_stationary_probability_frequentist_cdf() {
        let rm = RAND_MAX as u32;
        test_random_cdf(FrequentistCDF16::default(),
                        [(0,1), (0,1), (1,16), (0,1),
                         (1,32), (1,32), (0,1), (0,1),
                         (1,8), (0,1), (0,1), (0,1),
                         (1,5), (1,5), (1,5), (3,20)],
                        1000000);
    }
    #[test]
    fn test_stationary_probability_opt_frequentist_cdf() {
        let rm = RAND_MAX as u32;
        test_random_cdf(OptFrequentistCDF16::default(),
                        [(0,1), (0,1), (1,16), (0,1),
                         (1,32), (1,32), (0,1), (0,1),
                         (1,8), (0,1), (0,1), (0,1),
                         (1,5), (1,5), (1,5), (3,20)],
                        1000000);
    }
    #[cfg(feature="simd")]
    #[test]
    fn test_stationary_probability_simd_frequentist_cdf() {
        let rm = RAND_MAX as u32;
        test_random_cdf(SIMDFrequentistCDF16::default(),
                        [(0,1), (0,1), (1,16), (0,1),
                         (1,32), (1,32), (0,1), (0,1),
                         (1,8), (0,1), (0,1), (0,1),
                         (1,5), (1,5), (1,5), (3,20)],
                        1000000);
    }
    #[cfg(feature="debug_entropy")]
    #[test]
    fn test_stationary_probability_debug_cdf() {
        let rm = RAND_MAX as u32;
        let wrapper_cdf = test_random_cdf(super::DebugWrapperCDF16::<FrequentistCDF16>::default(),
                                          [(0,1), (0,1), (1,16), (0,1),
                                           (1,32), (1,32), (0,1), (0,1),
                                           (1,8), (0,1), (0,1), (0,1),
                                           (1,5), (1,5), (1,5), (3,20)],
                                          1000000);
        assert!(wrapper_cdf.num_samples().is_some());
        assert_eq!(wrapper_cdf.num_samples().unwrap(), 1000000);
    }
    #[test]
    fn test_blend_cdf_nonzero_pdf() {
        // This is a regression test
        let mut prob_state = BlendCDF16::default();
        for n in 0..1000000 {
            prob_state.blend(15, Speed::MED);
        }
        for i in 0..14 {
            assert!(prob_state.pdf(i) > 0);
        }
    }
}
