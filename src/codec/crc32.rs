/*
Copyright 2017 Andrew Gallant (BurntSushi)

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

    THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */
#[allow(unused_imports)]
use core;
use super::crc32_table::TABLE16;
pub fn crc32c_init() -> u32 {
    0
}
#[cfg(not(all(feature="simd", any(target_arch="x86", target_arch="x86_64"))))]
#[inline(always)]
pub fn crc32c_update(crc:u32, buf: &[u8]) -> u32 {
    fallback_crc32c_update(crc, buf)
}

#[cfg(all(feature="simd", any(target_arch="x86", target_arch="x86_64")))]
#[inline(always)]
pub fn crc32c_update(crc:u32, buf: &[u8]) -> u32 {
    if is_x86_feature_detected!("sse4.2") {
        return unsafe {
            sse_crc32c_update(crc, buf)
        };
    }
    fallback_crc32c_update(crc, buf)
}

#[inline(always)]
pub fn fallback_crc32c_update(mut crc:u32, mut buf: &[u8]) -> u32 {
    crc = !crc;
    while buf.len() >= 16 {
        crc ^= u32::from(buf[0]) | (u32::from(buf[1]) << 8) | (u32::from(buf[2]) << 16) | (u32::from(buf[3]) << 24);
        crc = TABLE16[0][buf[15] as usize]
            ^ TABLE16[1][buf[14] as usize]
            ^ TABLE16[2][buf[13] as usize]
            ^ TABLE16[3][buf[12] as usize]
            ^ TABLE16[4][buf[11] as usize]
            ^ TABLE16[5][buf[10] as usize]
            ^ TABLE16[6][buf[9] as usize]
            ^ TABLE16[7][buf[8] as usize]
            ^ TABLE16[8][buf[7] as usize]
            ^ TABLE16[9][buf[6] as usize]
            ^ TABLE16[10][buf[5] as usize]
            ^ TABLE16[11][buf[4] as usize]
            ^ TABLE16[12][(crc >> 24) as u8 as usize]
            ^ TABLE16[13][(crc >> 16) as u8 as usize]
            ^ TABLE16[14][(crc >> 8 ) as u8 as usize]
            ^ TABLE16[15][(crc      ) as u8 as usize];
        buf = &buf.split_at(16).1;
    }
    for &b in buf {
        crc = TABLE16[0][((crc as u8) ^ b) as usize] ^ (crc >> 8);
    }
    !crc
}
#[cfg(feature="simd")]
#[cfg(not(target_arch = "x86_64"))]
fn sse_crc32c_update(_crc:u32, _buf: &[u8]) -> u32 {
  unimplemented!();
}
#[cfg(feature="simd")]
#[cfg(target_arch = "x86_64")]
#[inline(always)]
//#[target_feature(enable = "sse4.2")]
unsafe fn sse_crc32c_update(mut crc:u32, mut buf: &[u8]) -> u32 {
    crc = !crc;
    while buf.len() >= 8 {
        crc = core::arch::x86_64::_mm_crc32_u64(u64::from(crc),
                                                u64::from(buf[0]) | (u64::from(buf[1]) << 8) | (u64::from(buf[2]) << 16) | (u64::from(buf[3]) << 24)
                                                |(u64::from(buf[4])<<32) | (u64::from(buf[5]) << 40) | (u64::from(buf[6]) << 48) | (u64::from(buf[7]) << 56)) as u32;
        buf = &buf.split_at(8).1;
    }
    for &b in buf {
        crc = core::arch::x86_64::_mm_crc32_u8(crc, b);
    }
    !crc
  }
mod test {
    #[cfg(test)]
    use super::{crc32c_init, crc32c_update};
    #[test]
    fn test_crc32c_empty() {
        assert_eq!(crc32c_update(crc32c_init(), &[]), 0x0);
    }
    #[test]
    fn test_crc32c_numeric() {
        let slice = b"123456789";
        assert_eq!(crc32c_update(crc32c_init(), slice), 0xe3069283);
    }
    #[test]
    fn test_crc32c_numeric_half() {
        let slice = b"123456789";
        let (firsthalf, secondhalf) = slice.split_at(5);
        assert_eq!(crc32c_update(crc32c_update(crc32c_init(), firsthalf), secondhalf), 0xe3069283);
    }
    #[test]
    fn test_crc32c_qbf() {
        let slice = b"The quick brown fox jumps over the lazy dog";
        assert_eq!(crc32c_update(crc32c_init(), slice), 0x22620404);
    }
    #[test]
    fn test_crc32c_qbf_half() {
        let slice = b"The quick brown fox jumps over the lazy dog";
        let (firsthalf, secondhalf) = slice.split_at(18);
        assert_eq!(crc32c_update(crc32c_update(crc32c_init(), firsthalf), secondhalf), 0x22620404);
    }
}
