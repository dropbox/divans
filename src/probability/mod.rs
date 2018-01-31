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
pub mod div_lut;
pub mod numeric;

#[macro_use]
mod common_tests;
pub mod interface;
pub mod external_cdf;
pub mod blend_cdf;
pub mod frequentist_cdf;
mod variant_speed_cdf;
#[cfg(feature="simd")]
pub mod simd_frequentist_cdf;
pub mod opt_frequentist_cdf;

pub use self::interface::{BaseCDF, CDF16, CDF2, Speed, SpeedPalette, Prob, LOG2_SCALE, BLEND_FIXED_POINT_PRECISION, ProbRange, SPEED_PALETTE_SIZE};
#[cfg(feature="debug_entropy")]
pub use self::interface::DebugWrapperCDF16;
pub use self::blend_cdf::{BlendCDF16};
pub use self::frequentist_cdf::FrequentistCDF16;
pub use self::external_cdf::ExternalProbCDF16;
#[cfg(feature="simd")]
pub use self::simd_frequentist_cdf::SIMDFrequentistCDF16;
pub use self::opt_frequentist_cdf::OptFrequentistCDF16;
pub use self::variant_speed_cdf::VariantSpeedCDF;
