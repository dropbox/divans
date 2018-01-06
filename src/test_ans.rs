#![cfg(test)]
#![allow(dead_code)]
extern crate std;

#[allow(unused_imports)]
use std::io::Write;

use core;
use super::ans::{
    ANSDecoder,
    ANSEncoder,
};
use super::probability::{Speed, BaseCDF, CDF16, BLEND_FIXED_POINT_PRECISION};
use ::DefaultCDF16;
use arithmetic_coder::{
    EntropyEncoder,
    EntropyDecoder,
    ByteQueue,
};
use interface::{
    NewWithAllocator,
};
#[allow(unused_imports)]
use alloc::{
    Allocator,
    SliceWrapper,
    SliceWrapperMut,
};
use super::codec::weights::Weights;
macro_rules! perror(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);

use super::test_helper::HeapAllocator;
const BITS: u8 = 8;
fn init_shuffle_256(src: &mut [u8]) -> u8 {
    let shuffled = [133, 240, 232, 124, 145, 29, 201, 207, 244, 226, 199, 176, 13, 173, 98, 179, 247, 69, 167, 6, 41, 117, 131, 44, 158, 38, 139, 253, 71, 250, 1, 101, 126, 65, 113, 57, 25, 97, 56, 16, 87, 64, 47, 138, 150, 212, 155, 0, 89, 118, 218, 68, 241, 77, 49, 112, 142, 143, 245, 48, 12, 152, 14, 195, 234, 95, 185, 37, 108, 137, 55, 63, 81, 120, 107, 34, 11, 52, 96, 111, 127, 189, 35, 223, 249, 221, 23, 154, 242, 136, 93, 141, 3, 84, 99, 248, 206, 62, 134, 211, 51, 216, 162, 61, 183, 72, 198, 40, 122, 202, 190, 163, 180, 171, 153, 159, 166, 186, 164, 210, 91, 165, 213, 30, 15, 33, 27, 172, 104, 121, 147, 219, 140, 36, 4, 28, 43, 45, 102, 24, 5, 168, 188, 114, 255, 160, 209, 181, 21, 182, 130, 254, 214, 83, 170, 82, 105, 187, 192, 156, 26, 196, 184, 54, 116, 46, 228, 115, 19, 76, 169, 225, 32, 10, 193, 60, 215, 103, 22, 42, 144, 80, 161, 78, 17, 94, 2, 31, 18, 203, 129, 20, 9, 227, 246, 224, 229, 135, 231, 73, 66, 125, 230, 119, 151, 67, 86, 205, 128, 174, 243, 74, 123, 92, 191, 110, 157, 106, 100, 70, 148, 237, 132, 109, 220, 53, 8, 197, 50, 175, 251, 208, 204, 79, 146, 149, 222, 178, 233, 58, 252, 217, 177, 7, 235, 236, 59, 194, 75, 85, 90, 238, 200, 239, 88, 39];
    for (s,v) in shuffled.iter().cycle().zip(src.iter_mut()) {
        *v = *s;
    }
    127
}
fn init_shuffle_384(src: &mut [u8]) -> u8 {
    let shuffled = [133, 240, 232, 124, 145, 29, 201, 207, 244, 226, 199, 176, 13, 173, 98, 179, 247, 69, 167, 6, 41, 117, 131, 44, 158, 38, 139, 253, 71, 250, 1, 101, 126, 65, 113, 57, 25, 97, 56, 16, 87, 64, 47, 138, 150, 212, 155, 0, 89, 118, 218, 68, 241, 77, 49, 112, 142, 143, 245, 48, 12, 152, 14, 195, 234, 95, 185, 37, 108, 137, 55, 63, 81, 120, 107, 34, 11, 52, 96, 111, 127, 189, 35, 223, 249, 221, 23, 154, 242, 136, 93, 141, 3, 84, 99, 248, 206, 62, 134, 211, 51, 216, 162, 61, 183, 72, 198, 40, 122, 202, 190, 163, 180, 171, 153, 159, 166, 186, 164, 210, 91, 165, 213, 30, 15, 33, 27, 172, 104, 121, 147, 219, 140, 36, 4, 28, 43, 45, 102, 24, 5, 168, 188, 114, 255, 160, 209, 181, 21, 182, 130, 254, 214, 83, 170, 82, 105, 187, 192, 156, 26, 196, 184, 54, 116, 46, 228, 115, 19, 76, 169, 225, 32, 10, 193, 60, 215, 103, 22, 42, 144, 80, 161, 78, 17, 94, 2, 31, 18, 203, 129, 20, 9, 227, 246, 224, 229, 135, 231, 73, 66, 125, 230, 119, 151, 67, 86, 205, 128, 174, 243, 74, 123, 92, 191, 110, 157, 106, 100, 70, 148, 237, 132, 109, 220, 53, 8, 197, 50, 175, 251, 208, 204, 79, 146, 149, 222, 178, 233, 58, 252, 217, 177, 7, 235, 236, 59, 194, 75, 85, 90, 238, 200, 239, 88, 39, 133, 240, 232, 124, 145, 29, 201, 207, 244, 226, 199, 176, 13, 173, 98, 179, 247, 69, 167, 6, 41, 117, 131, 44, 158, 38, 139, 253, 71, 250, 1, 101, 126, 65, 113, 57, 25, 97, 56, 16, 87, 64, 47, 138, 150, 212, 155, 0, 89, 118, 218, 68, 241, 77, 49, 112, 142, 143, 245, 48, 12, 152, 14, 195, 234, 95, 185, 37, 108, 137, 55, 63, 81, 120, 107, 34, 11, 52, 96, 111, 127, 189, 35, 223, 249, 221, 23, 154, 242, 136, 93, 141, 3, 84, 99, 248, 206, 62, 134, 211, 51, 216, 162, 61, 183, 72, 198, 40, 122, 202, 190, 163, 180, 171, 153, 159, 166, 186, 164, 210, 91, 165, 213, 30, 15, 33, 27, 172];
    for (s,v) in shuffled.iter().cycle().zip(src.iter_mut()) {
        *v = *s;
    }
    127
}
fn init_src(src: &mut [u8]) -> u8 {
    let mut ones = 0u64;
    let seed: [u8; 16] = [0xef, 0xbf,0xff,0xfd,0xef,0x3f,0xc0,0xfd,0xef,0xc0,0xff,0xfd,0xdf,0x3f,0xff,0xfd,
    ];
    for (s,v) in seed.iter().cycle().zip(src.iter_mut()) {
        *v = *s;
    }
    for v in src.iter() {
        for i in 0..8 {
            if 1u8<<i & v != 0 {
                ones = ones + 1;
            }
        }
    }
    ((ones<<BITS) as u64 / (src.len() as u64 * 8)) as u8
}

fn make_test_cdfs() -> (DefaultCDF16, [DefaultCDF16; 16]) {
    (DefaultCDF16::default(),
     [
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default(),
         DefaultCDF16::default()])
}

trait TestSelection : Clone + Copy {
    type C16: BaseCDF+CDF16;
    fn size(&self) -> usize;
    fn adapt_probability(&self) -> bool;
    fn independent_hilo(&self) -> bool;
    fn make_test_cdfs(&self) -> (Self::C16, [Self::C16; 16]);
    fn adaptive_context_mixing(&self) -> bool;
    fn two_models(&self) -> bool;
}
#[derive(Clone, Copy)]
struct TestContextMixing{
    pub size: usize,
}
#[derive(Clone, Copy)]
struct TestContextMixingPureAverage{
    pub size: usize,
}
#[derive(Clone, Copy)]
struct TestAdapt{
    pub size: usize,
}
#[derive(Clone, Copy)]
struct TestNoAdapt{
    pub size: usize,
}
#[derive(Clone, Copy)]
struct TestSimple{
    pub size: usize,
}
impl TestSelection for TestContextMixing {
    type C16 = DefaultCDF16;
    fn size(&self) -> usize {self.size}
    fn adapt_probability(&self) -> bool {true}
    fn adaptive_context_mixing(&self) -> bool {true}
    fn independent_hilo(&self) -> bool {false}
    fn two_models(&self) -> bool {true}
    fn make_test_cdfs(&self) -> (DefaultCDF16, [DefaultCDF16; 16]) {
        self::make_test_cdfs()
    }
}

impl TestSelection for TestContextMixingPureAverage {
    type C16 = DefaultCDF16;
    fn size(&self) -> usize {self.size}
    fn adapt_probability(&self) -> bool {true}
    fn adaptive_context_mixing(&self) -> bool {false}
    fn independent_hilo(&self) -> bool {false}
    fn two_models(&self) -> bool {true}
    fn make_test_cdfs(&self) -> (DefaultCDF16, [DefaultCDF16; 16]) {
        self::make_test_cdfs()
    }
}
impl TestSelection for TestAdapt {
    type C16 = DefaultCDF16;
    fn size(&self) -> usize {self.size}
    fn adapt_probability(&self) -> bool {true}
    fn adaptive_context_mixing(&self) -> bool {false}
    fn independent_hilo(&self) -> bool {false}
    fn two_models(&self) -> bool {false}
    fn make_test_cdfs(&self) -> (DefaultCDF16, [DefaultCDF16; 16]) {
        self::make_test_cdfs()
    }
}
impl TestSelection for TestNoAdapt {
    type C16 = DefaultCDF16;
    fn size(&self) -> usize {self.size}
    fn adapt_probability(&self) -> bool {false}
    fn adaptive_context_mixing(&self) -> bool {false}
    fn independent_hilo(&self) -> bool {false}
    fn two_models(&self) -> bool {false}
    fn make_test_cdfs(&self) -> (DefaultCDF16, [DefaultCDF16; 16]) {
        self::make_test_cdfs()
    }
}

impl TestSelection for TestSimple {
    type C16 = DefaultCDF16;
    fn size(&self) -> usize {self.size}
    fn adapt_probability(&self) -> bool {false}
    fn adaptive_context_mixing(&self) -> bool {false}
    fn independent_hilo(&self) -> bool {true}
    fn two_models(&self) -> bool {false}
    fn make_test_cdfs(&self) -> (DefaultCDF16, [DefaultCDF16; 16]) {
        self::make_test_cdfs()
    }
}
#[inline(always)]
fn encode_test_nibble_helper<AllocU8: Allocator<u8>,
                             TS:TestSelection>(e: &mut ANSEncoder<AllocU8>, src: &[u8], dst: &mut [u8], n: &mut usize, ts: TS) {
    let mut t = 0;
    *n = 0;
    let mut weights = [Weights::default(), Weights::default()];
    let (mut cdf_high, mut cdf_low) = ts.make_test_cdfs();
    let (mut _unused, mut cdf_low_adv) = ts.make_test_cdfs();
    let (mut _unused, mut cdf_high_adv) = ts.make_test_cdfs();
    let mut last_nibble = 0usize;
    for u in src.iter() {
        let v = *u;
        let blend0 = if ts.adaptive_context_mixing() {
            weights[0].norm_weight() as u16 as i32
        } else {
            1 << (BLEND_FIXED_POINT_PRECISION - 2)
        };
        let blend1 = if ts.adaptive_context_mixing() {
            weights[1].norm_weight() as u16 as i32
        } else {
            1 << (BLEND_FIXED_POINT_PRECISION - 2)
        };
        //left to right
        let high_nibble = v >> 4;
        let high_weighted_prob_range = if ts.two_models(){
            e.put_nibble(high_nibble, &cdf_high.average(&cdf_high_adv[last_nibble], blend0))
        } else {
            e.put_nibble(high_nibble, &cdf_high_adv[last_nibble])
        };
        if ts.adaptive_context_mixing() {
            let model_probs = [
                cdf_high.sym_to_start_and_freq(high_nibble).range.freq,
                cdf_high_adv[last_nibble].sym_to_start_and_freq(high_nibble).range.freq,
            ];
            weights[0].update(model_probs,
                              high_weighted_prob_range.freq);
        }

        {
            let mut q = e.get_internal_buffer();
            let qb = q.num_pop_bytes_avail();
            if qb > 0 {
                assert!(qb + *n <= dst.len());
                q.pop_data(&mut dst[*n  .. *n + qb]);
                *n = *n + qb;
            }
        }
        if ts.adapt_probability() {
            if ts.two_models() {
                cdf_high.blend(v >> 4, Speed::SLOW);
            }
            cdf_high_adv[last_nibble].blend(v >> 4, Speed::MED);
        }
        let cdfl = &mut cdf_low[if ts.independent_hilo() {0} else {(v>>4) as usize}];
        let low_nibble = v & 0xf;

        let low_weighted_prob_range = if ts.two_models(){
            e.put_nibble(low_nibble, &cdfl.average(&cdf_low_adv[last_nibble], blend1))
        } else {
            e.put_nibble(low_nibble, &cdf_low_adv[last_nibble])
        };
        let mut q = e.get_internal_buffer();
        let qb = q.num_pop_bytes_avail();
        if qb > 0 {
            assert!(qb + *n <= dst.len());
            q.pop_data(&mut dst[*n  .. *n + qb]);
            *n = *n + qb;
        }
        if ts.adaptive_context_mixing() {
            let model_probs = [
                cdfl.sym_to_start_and_freq(low_nibble).range.freq,
                cdf_low_adv[last_nibble].sym_to_start_and_freq(low_nibble).range.freq,
            ];
            weights[1].update(model_probs,
                              low_weighted_prob_range.freq);
        }
        if ts.adapt_probability() {
            if ts.two_models() {
                cdfl.blend(v & 0xf, Speed::SLOW);
            }
            cdf_low_adv[last_nibble].blend(v & 0xf, Speed::SLOW);
        }
        last_nibble = v as usize & 0xf;
        t += 1;
    }
    assert_eq!(t, src.len());
    e.flush();
    {
        let q = e.get_internal_buffer();
        let qb = q.num_pop_bytes_avail();
        q.pop_data(&mut dst[*n .. *n + qb]);
        *n = *n + qb;
    }
}
#[inline(always)]
fn decode_test_nibble_helper<AllocU8: Allocator<u8>,
                             TS:TestSelection>(d: &mut ANSDecoder, src: &[u8], n: &mut usize, end: &mut [u8], ts: TS) {
    let mut weights = [Weights::default(), Weights::default()];
    let max_copy =1024usize;
    let (mut cdf_high, mut cdf_low) = ts.make_test_cdfs();
    let (mut _unused, mut cdf_low_adv) = ts.make_test_cdfs();
    let (mut _unused, mut cdf_high_adv) = ts.make_test_cdfs();
    let mut last_nibble = 0usize;
    let mut t = 0;
    {
        let q = d.get_internal_buffer();
        let sz = q.num_push_bytes_avail();
        //assert!(sz >= 10);
        //assert!(sz <= 16);
        assert!(src.len() >= sz);
        let p = q.push_data(&src[*n  .. *n + sz]);
        assert_eq!(p, sz);
        //assert!(q.num_pop_bytes_avail() == sz);
        *n = *n + sz;
    }
    let mut blend0 = if ts.adaptive_context_mixing() {
        weights[0].norm_weight() as u16 as i32
    } else {
        1 << (BLEND_FIXED_POINT_PRECISION - 2)
    };
    for v in end.iter_mut() {
        let blend1 = if ts.adaptive_context_mixing() {
            weights[1].norm_weight() as u16 as i32
        } else {
            1 << (BLEND_FIXED_POINT_PRECISION - 2)
        };
        let (high_nibble, high_weighted_prob_range) = if ts.two_models() {
            d.get_nibble(&cdf_high.average(&cdf_high_adv[last_nibble], blend0))
        } else {
            d.get_nibble(&cdf_high_adv[last_nibble])
        };
        *v = high_nibble << 4;
        if ts.adaptive_context_mixing() {
            let model_probs = [
                cdf_high.sym_to_start_and_freq(high_nibble).range.freq,
                cdf_high_adv[last_nibble].sym_to_start_and_freq(high_nibble).range.freq,
            ];
            weights[0].update(model_probs,
                              high_weighted_prob_range.freq);
        }
        {
            let mut q = d.get_internal_buffer();
            while q.num_push_bytes_avail() > 0 {
                let sz = core::cmp::min(core::cmp::min(src.len() - *n, q.num_push_bytes_avail()),
                                        max_copy);
                q.push_data(&src[*n .. *n + sz]);
                *n = *n + sz;
            }
        }
        let cdfl = &mut cdf_low[if ts.independent_hilo() {0}else{(*v >> 4) as usize}];
        if ts.adapt_probability() {
            if ts.two_models() {
                cdf_high.blend(*v >> 4, Speed::SLOW);
            }
            cdf_high_adv[last_nibble].blend(*v >> 4, Speed::MED);
        }
        let (low_nibble, low_weighted_prob_range) = if ts.two_models() {
            d.get_nibble(&cdfl.average(&cdf_low_adv[last_nibble], blend1))
        } else {
            d.get_nibble(&cdf_low_adv[last_nibble])
        };
        *v |= low_nibble;
        let mut q = d.get_internal_buffer();
        while q.num_push_bytes_avail() > 0 {
            let sz = core::cmp::min(core::cmp::min(src.len() - *n, q.num_push_bytes_avail()),
                                    max_copy);
            q.push_data(&src[*n .. *n + sz]);
            *n = *n + sz;
        }
        blend0 = if ts.adaptive_context_mixing() {
            weights[0].norm_weight() as u16 as i32
        } else {
            1 << (BLEND_FIXED_POINT_PRECISION - 2)
        };
        if ts.adaptive_context_mixing() {
            let model_probs = [
                cdfl.sym_to_start_and_freq(low_nibble).range.freq,
                cdf_low_adv[last_nibble].sym_to_start_and_freq(low_nibble).range.freq,
            ];
            weights[1].update(model_probs,
                              low_weighted_prob_range.freq);
        }
        if ts.adapt_probability() {
            if ts.two_models() {
                cdfl.blend(low_nibble, Speed::SLOW);
            }
            cdf_low_adv[last_nibble].blend(low_nibble, Speed::SLOW);
        }
        last_nibble = *v as usize & 0xf;
        t = t + 1;
    }
    assert_eq!(t, end.len());
}


fn encode_test_helper<AllocU8: Allocator<u8>>(e: &mut ANSEncoder<AllocU8>, p0: u8, src: &[u8], dst: &mut [u8], n: &mut usize, trailer: bool) {
    let mut t = 0;
    *n = 0;
    for u in src.iter() {
        let v = *u;
        //left to right
        for i in (0..8).rev() {
            let b: bool = (v & (1u8<<i)) != 0;
            e.put_bit(b, p0);
            let mut q = e.get_internal_buffer();
            let qb = q.num_pop_bytes_avail();
            if qb > 0 {
                assert!(qb + *n <= dst.len());
                q.pop_data(&mut dst[*n  .. *n + qb]);
                *n = *n + qb;
            }
            t = t + 1;
        }
    }
    assert!(t == src.len() * 8);
    if trailer {
        e.put_bit(true, 1);
        {
            let q = e.get_internal_buffer();
            let qb = q.num_pop_bytes_avail();
            if qb > 0 {
                assert!(qb + *n <= dst.len());
                q.pop_data(&mut dst[*n  .. *n + qb]);
                *n = *n + qb;
            }
        }
        e.put_bit(false, 1);
        {
            let q = e.get_internal_buffer();
            let qb = q.num_pop_bytes_avail();
            if qb > 0 {
                assert!(qb + *n <= dst.len());
                q.pop_data(&mut dst[*n  .. *n + qb]);
                *n = *n + qb;
            }
        }
    }
    e.flush();
    {
        let q = e.get_internal_buffer();
        let qb = q.num_pop_bytes_avail();
        q.pop_data(&mut dst[*n .. *n + qb]);
        *n = *n + qb;
    }
}

fn decode_test_helper<AllocU8: Allocator<u8>>(d: &mut ANSDecoder, p0: u8, src: &[u8], n: &mut usize, end: &mut [u8], trailer: bool) {
    let max_copy = if trailer {1usize} else {1024usize};
    let mut t = 0;
    {
        let q = d.get_internal_buffer();
        let sz = q.num_push_bytes_avail();
        //assert!(sz >= 10);
        //assert!(sz <= 16);
        assert!(src.len() >= sz);
        let p = q.push_data(&src[*n  .. *n + sz]);
        assert!(p == sz);
        //assert!(q.num_pop_bytes_avail() == sz);
        *n = *n + sz;
    }
    for v in end.iter_mut() {
        *v = 0;
        for b in 0..8 {
            let bit = d.get_bit(p0);
            if bit {
                *v = *v | (1u8<<(7 - b));
            }
            let mut q = d.get_internal_buffer();
            while q.num_push_bytes_avail() > 0 && *n < src.len() {
                let sz = core::cmp::min(core::cmp::min(src.len() - *n, q.num_push_bytes_avail()),
                                        max_copy);
                q.push_data(&src[*n .. *n + sz]);
                *n = *n + sz;
            }
            t = t + 1;
        }
    }
    assert!(t == 8*end.len());
    if trailer {
        let bit = d.get_bit(1);
        assert!(bit);
        {
            let q = d.get_internal_buffer();
            while q.num_push_bytes_avail() > 0 && *n < src.len() {
                let sz = core::cmp::min(core::cmp::min(src.len() - *n, q.num_push_bytes_avail()),
                                        max_copy);
                q.push_data(&src[*n .. *n + sz]);
                *n = *n + sz;
            }
        }
        let bit = d.get_bit(1);
        assert!(!bit);
        let q = d.get_internal_buffer();
        while q.num_push_bytes_avail() > 0 && *n < src.len() {
            let sz = core::cmp::min(core::cmp::min(src.len() - *n, q.num_push_bytes_avail()),
                                    max_copy);
            q.push_data(&src[*n .. *n + sz]);
            *n = *n + sz;
        }
    }
}
#[cfg(feature="benchmark")]
extern crate test;
#[cfg(feature="benchmark")]
use self::test::Bencher;
#[cfg(feature="benchmark")]
#[bench]
fn entropy_bit_roundtrip_100k(b: &mut Bencher) {
    const SZ: usize = 1024*1024 / 10;
    let mut m8 = HeapAllocator::<u8>{default_value: 0u8};
    let mut src = m8.alloc_cell(SZ);
    let mut dst = m8.alloc_cell(SZ);
    let mut start = m8.alloc_cell(SZ);
    let mut end = m8.alloc_cell(SZ);
    let (prob0, _optimal) = setup_test_return_optimal(
        src.slice_mut(), dst.slice_mut(), end.slice_mut(), start.slice_mut(), init_src);
    let mut compressed_size = 0;
    let mut actual = 1.0f64;
    b.iter(|| {
        let mut n: usize = 0;
        let mut d = ANSDecoder::new(&mut m8);
        let mut e = ANSEncoder::new(&mut m8);
        encode_test_helper(&mut e, prob0, src.slice(), dst.slice_mut(), &mut n, false);
        compressed_size = n;
        let nbits = n * 8;
        actual = nbits as f64;
        //assert!(actual >= _optimal);
        n = 0;
        decode_test_helper::<HeapAllocator<u8>>(&mut d, prob0, dst.slice(), &mut n, end.slice_mut(), false);
    });
    perror!("encoded size: {}", compressed_size);
    perror!("effeciency: {}", actual / _optimal);
    let mut t = 0;
    for (e,s) in end.slice().iter().zip(start.slice().iter()) {
        assert!(e == s, "byte {} mismatch {:b} != {:b} ", t, e, s);
        t = t + 1;
    }
    assert!(t == SZ);
    perror!("done!");
}
#[cfg(feature="benchmark")]
#[bench]
fn entropy_bit_decode_bench_100k(b: &mut Bencher) {
    const SZ: usize = 1024*1024 / 10;
    let mut m8 = HeapAllocator::<u8>{default_value: 0u8};
    let mut src = m8.alloc_cell(SZ);
    let mut dst = m8.alloc_cell(SZ );
    let mut end = m8.alloc_cell(SZ);
    let mut start = m8.alloc_cell(SZ);
    let (prob0, _optimal) = setup_test_return_optimal(
        src.slice_mut(), dst.slice_mut(), end.slice_mut(), start.slice_mut(), init_src);
    let mut e = ANSEncoder::new(&mut m8);
    let mut n: usize = 0;
    encode_test_helper(&mut e, prob0, src.slice(), dst.slice_mut(), &mut n, false);
    perror!("encoded size: {}", n);
    let nbits = n * 8;
    let actual = nbits as f64;
    let _unused = actual;
    perror!("effeciency: {}", actual / _optimal);
    b.iter(|| {
        let mut d = ANSDecoder::new(&mut m8);
        //assert!(actual >= _optimal);
        n = 0;
        decode_test_helper::<HeapAllocator<u8>>(&mut d, prob0, dst.slice(), &mut n, end.slice_mut(), false);
    });
    let mut t = 0;
    for (e,s) in end.slice().iter().zip(start.slice().iter()) {
        assert!(e == s, "byte {} mismatch {:b} != {:b} ", t, e, s);
        t = t + 1;
    }
    assert!(t == SZ);
    perror!("done!");
}

#[cfg(feature="benchmark")]
#[bench]
fn nibble_encode_roundtrip_context_mixing_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_roundtrip(b, TestContextMixing{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_encode_roundtrip_context_pure_average_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_roundtrip(b, TestContextMixingPureAverage{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_encode_roundtrip_model_adapt_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_roundtrip(b, TestAdapt{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_encode_roundtrip_nonadaptive_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_roundtrip(b, TestNoAdapt{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_encode_roundtrip_simple_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_roundtrip(b, TestSimple{size:1024 * 1024/10})
}


#[cfg(feature="benchmark")]
#[bench]
fn nibble_encode_only_context_mixing_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_encode_only(b, TestContextMixing{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_encode_only_context_pure_average_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_encode_only(b, TestContextMixingPureAverage{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_encode_only_model_adapt_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_encode_only(b, TestAdapt{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_encode_only_nonadaptive_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_encode_only(b, TestNoAdapt{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_encode_only_simple_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_encode_only(b, TestSimple{size:1024 * 1024/10})
}

#[cfg(feature="benchmark")]
#[bench]
fn nibble_decode_context_mixing_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_decode(b, TestContextMixing{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_decode_context_pure_average_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_decode(b, TestContextMixingPureAverage{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_decode_model_adapt_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_decode(b, TestAdapt{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_decode_nonadaptive_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_decode(b, TestNoAdapt{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
#[bench]
fn nibble_decode_simple_100k(b: &mut Bencher) {
    entropy_dynamic_nibble_decode(b, TestSimple{size:1024 * 1024/10})
}
#[cfg(feature="benchmark")]
fn entropy_dynamic_nibble_decode<TS:TestSelection>(b: &mut Bencher, ts:TS) {
    let sz: usize = ts.size();
    let mut m8 = HeapAllocator::<u8>{default_value: 0u8};
    let mut src = m8.alloc_cell(sz);
    let mut dst = m8.alloc_cell(sz * 2);
    let mut end = m8.alloc_cell(sz);
    let mut start = m8.alloc_cell(sz);
    let (_prob0, _optimal) = setup_test_return_optimal(
        src.slice_mut(), dst.slice_mut(), end.slice_mut(), start.slice_mut(), init_shuffle_384);
    let mut e = ANSEncoder::new(&mut m8);
    let mut n: usize = 0;
    encode_test_nibble_helper(&mut e, src.slice(), dst.slice_mut(), &mut n, ts);
    perror!("encoded size: {}", n);
    let nbits = n * 8;
    let actual = nbits as f64;
    let _unused = actual;
    perror!("effeciency: {}", actual / _optimal);
    let mut random_number = 1usize;
    b.iter(|| {
        let mut d = ANSDecoder::new(&mut m8);
        //assert!(actual >= _optimal);
        n = 0;
        decode_test_nibble_helper::<HeapAllocator<u8>, TS>(&mut d, dst.slice(), &mut n, end.slice_mut(), ts);
        assert_eq!(src[random_number % sz],
                   end[random_number % sz]);
        random_number = random_number.wrapping_add(1289421921488429);
    });
    let mut t = 0;
    for (e,s) in end.slice().iter().zip(start.slice().iter()) {
        assert!(e == s, "byte {} mismatch {:b} != {:b} ", t, e, s);
        t = t + 1;
    }
    assert!(t == sz);
    perror!("done!");
}


#[cfg(feature="benchmark")]
fn entropy_dynamic_nibble_roundtrip<TS:TestSelection>(b: &mut Bencher, ts:TS) {
    let sz: usize = ts.size();
    let mut m8 = HeapAllocator::<u8>{default_value: 0u8};
    let mut src = m8.alloc_cell(sz);
    let mut dst = m8.alloc_cell(sz * 2);
    let mut start = m8.alloc_cell(sz);
    let mut end = m8.alloc_cell(sz);
    let (_prob0, _optimal) = setup_test_return_optimal(
        src.slice_mut(), dst.slice_mut(), end.slice_mut(), start.slice_mut(), init_shuffle_256);
    let mut compressed_size = 0;
    let mut actual = 1.0f64;
    let _unused = actual;
    b.iter(|| {
        let mut n: usize = 0;
        let mut d = ANSDecoder::new(&mut m8);
        let mut e = ANSEncoder::new(&mut m8);
        encode_test_nibble_helper(&mut e, src.slice(), dst.slice_mut(), &mut n, ts);
        compressed_size = n;
        let nbits = n * 8;
        actual = nbits as f64;
        //assert!(actual >= _optimal);
        n = 0;
        decode_test_nibble_helper::<HeapAllocator<u8>, TS>(&mut d, dst.slice(), &mut n, end.slice_mut(), ts);
    });
    perror!("encoded size: {}", compressed_size);
    perror!("effeciency: {}", actual / _optimal);
    let mut t = 0;
    for (e,s) in end.slice().iter().zip(start.slice().iter()) {
        assert!(e == s, "byte {} mismatch {:b} != {:b} ", t, e, s);
        t = t + 1;
    }
    assert!(t == sz);
    perror!("done!");
}


#[cfg(feature="benchmark")]
fn entropy_dynamic_nibble_encode_only<TS:TestSelection>(b: &mut Bencher, ts:TS) {
    let sz: usize = ts.size();
    let mut m8 = HeapAllocator::<u8>{default_value: 0u8};
    let mut src = m8.alloc_cell(sz);
    let mut dst = m8.alloc_cell(sz * 2);
    let mut start = m8.alloc_cell(sz);
    let mut end = m8.alloc_cell(sz);
    let (_prob0, _optimal) = setup_test_return_optimal(
        src.slice_mut(), dst.slice_mut(), end.slice_mut(), start.slice_mut(), init_shuffle_256);
    let mut compressed_size = 0;
    let mut actual = 1.0f64;
    let _unused = actual;
    b.iter(|| {
        let mut n: usize = 0;
        let mut e = ANSEncoder::new(&mut m8);
        encode_test_nibble_helper(&mut e, src.slice(), dst.slice_mut(), &mut n, ts);
        compressed_size = n;
        let nbits = n * 8;
        actual = nbits as f64;
        //assert!(actual >= _optimal);
    });
    let mut d = ANSDecoder::new(&mut m8);
    let mut n = 0usize;
    decode_test_nibble_helper::<HeapAllocator<u8>, TS>(&mut d, dst.slice(), &mut n, end.slice_mut(), ts);
    perror!("encoded size: {}", compressed_size);
    perror!("effeciency: {}", actual / _optimal);
    let mut t = 0;
    for (e,s) in end.slice().iter().zip(start.slice().iter()) {
        assert!(e == s, "byte {} mismatch {:b} != {:b} ", t, e, s);
        t = t + 1;
    }
    assert!(t == sz);
    perror!("done!");
}



fn setup_test_return_optimal(src:&mut[u8], _dst:&mut[u8], _end:&mut [u8], start:&mut[u8], ini: fn (&mut [u8]) -> u8) -> (u8, f64) {
    let prob = ini(src);
    let prob0: u8 = ((1u64<<BITS) - (prob as u64)) as u8;
    let z = src.len() as f64 * 8.0;
    let p1 = prob as f64 / 256.0;
    let p0 = 1.0 - p1;
    start.clone_from_slice(src.iter().as_slice());
    (prob0, -1.0 * p1.log2() * (p1 * z) + (-1.0) * p0.log2() * (p0 * z))
}
fn help_rt(src:&mut[u8], dst:&mut[u8], end:&mut [u8], start:&mut[u8], trailing_bit_and_one_byte_at_a_time: bool) {
    let sz = src.len();
    let mut m8 = HeapAllocator::<u8>{default_value: 0u8};
    let (prob0, _optimal) = setup_test_return_optimal(src, dst, end, start, init_src);
    let mut d = ANSDecoder::new(&mut m8);
    let mut e = ANSEncoder::new(&mut m8);
    let mut n: usize = 0;
    encode_test_helper(&mut e, prob0, src, dst, &mut n, trailing_bit_and_one_byte_at_a_time);
    perror!("encoded size: {}", n);

    let nbits = n * 8;
    let actual = nbits as f64;
    let _unused = actual;
    perror!("effeciency: {}", actual / _optimal);
    //assert!(actual >= _optimal);
    n = 0;
    decode_test_helper::<HeapAllocator<u8>>(&mut d, prob0, dst, &mut n, end, trailing_bit_and_one_byte_at_a_time);
    let mut t = 0;
    for (e,s) in end.iter().zip(start.iter()) {
        assert!(e == s, "byte {} mismatch {:b} != {:b} ", t, e, s);
        t = t + 1;
    }
    assert!(t == sz);
    perror!("done!");        
}

#[test]
fn entropy16_trait_test() {
    const SZ: usize = 1024*4 - 4;
    let mut src: [u8; SZ] = [0; SZ];
    let mut dst: [u8; SZ + 16] = [0; SZ + 16];
    let mut end: [u8; SZ] = [0; SZ];
    let mut start = [0u8; SZ];
    help_rt(&mut src[..],&mut dst[..],&mut end[..],&mut start[..], false)
}

#[test]
fn entropy16_lite_trait_test() {
    const SZ: usize = 16;
    let mut src: [u8; SZ] = [0; SZ];
    let mut dst: [u8; SZ + 16] = [0; SZ + 16];
    let mut end: [u8; SZ] = [0; SZ];
    let mut start = [0u8; SZ];
    help_rt(&mut src[..],&mut dst[..],&mut end[..],&mut start[..], true)
}
#[test]
fn entropy16_big_trait_test() {
    const SZ: usize = 4097;
    let mut src: [u8; SZ] = [0; SZ];
    let mut dst: [u8; SZ + 16] = [0; SZ + 16];
    let mut end: [u8; SZ] = [0; SZ];
    let mut start = [0u8; SZ];
    help_rt(&mut src[..],&mut dst[..],&mut end[..],&mut start[..], true)
}
