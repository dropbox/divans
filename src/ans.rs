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

use core;
use alloc::{
    Allocator,
    SliceWrapper,
    SliceWrapperMut
};
use core::default::Default;
use core::{mem, cmp};
use probability::{CDF16, BaseCDF, Prob};
use super::interface::{
    ArithmeticEncoderOrDecoder,
    NewWithAllocator,
    BillingCapability,
};
use super::BrotliResult;
use super::encoder::{
    EntropyEncoder,
    EntropyDecoder,
    ByteQueue,
};


#[cfg(test)]
#[cfg(not(feature="benchmark"))]
macro_rules! perror(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);
/*
#[cfg(not(test))]
macro_rules! perror(
    ($($val:tt)*) => { {
    } }
);*/
#[cfg(test)]
#[cfg(feature="benchmark")]
macro_rules! perror(
    ($($val:tt)*) => { {
    } }
);

pub const MAX_BUFFER_SIZE: usize = 256*1024; // with space for size

pub struct ByteStack<AllocU8: Allocator<u8>>  {
    data : AllocU8::AllocatedMemory,
    nbytes : usize,
}

impl<A: Allocator<u8>> NewWithAllocator<A> for ByteStack<A> {
    fn new(m8: &mut A) -> Self {
        let data = m8.alloc_cell(MAX_BUFFER_SIZE);
        ByteStack {data: data, nbytes: MAX_BUFFER_SIZE}
    }
}

impl<AllocU8: Allocator<u8>> ByteStack<AllocU8> {
    pub fn reset(&mut self) {
        self.nbytes = self.data.slice().len();
    }
    pub fn bytes(&mut self) -> &[u8] {
        let sl = self.data.slice();
        &sl[self.nbytes ..  sl.len()]
    }
    pub fn stack_bytes_avail(&self) -> usize {
        self.nbytes
    }
    pub fn is_empty(&self) -> bool {
        self.nbytes == self.data.slice().len()
    }
    pub fn stack_data(&mut self, src: &[u8]) {
        for v in src.iter().rev() {
            self.stack_byte(*v);
        }
    }
    pub fn stack_byte(&mut self, b: u8) {
        assert!(self.nbytes > 0);
        self.nbytes -= 1;
        self.data.slice_mut()[self.nbytes] = b;
    }
    pub fn stack_u16(&mut self, s: u16) {
        self.stack_byte(((s >> 8) & 0xff) as u8);
        self.stack_byte((s & 0xff) as u8);
    }
}

impl<AllocU8: Allocator<u8>> ByteQueue for ByteStack<AllocU8> {
    fn num_push_bytes_avail(&self) -> usize {
        self.nbytes
    }
    fn num_pop_bytes_avail(&self) -> usize {
        self.data.slice().len() - self.nbytes
    }
    fn push_data(&mut self, _data:&[u8]) -> usize {
        assert!(false); //only pop from this queue
        0
    }
    fn pop_data(&mut self, data:&mut [u8]) -> usize {
        let n = core::cmp::min(data.len(), self.num_pop_bytes_avail());
        let sl = self.data.slice()[self.nbytes .. self.nbytes + n].iter();
        for (d, s) in data.iter_mut().zip(sl) {
            *d = *s;
        }
        self.nbytes += n;
        n
    }
}


type ANSState = u64;
type StartFreqType = Prob;
const NORMALIZATION_INTERVAL: ANSState = 1u64 << 31;
const ENC_START_STATE: ANSState = NORMALIZATION_INTERVAL;
const LOG2_SCALE: u32 = 15;
const NUM_SYMBOLS_BEFORE_FLUSH:u32 = (MAX_BUFFER_SIZE as u32) >> 2;
const SCALE_MASK:u64 = ((1u64 << LOG2_SCALE) - 1);

#[derive(Debug)]
pub struct ANSDecoder {
    state_a: u64,
    state_b: u64,
    sym_count: u16, // FIXME: this may be able to be a u16
    buffer_a_bytes_required: u8, // needs 8 to start with
    buffer_b_bytes_required: u8, // needs 8 to start with
}

impl Default for ANSDecoder {
    fn default() -> Self {
        let ret = ANSDecoder{
            state_a: 0,
            state_b: 0,
            sym_count: 0,
            buffer_a_bytes_required: 8, // this will load both buffers
            buffer_b_bytes_required: 0,
        };
        assert!((1 << (mem::size_of_val(&ret.sym_count) * 8)) >= NUM_SYMBOLS_BEFORE_FLUSH);
        ret
    }
}

impl<A: Allocator<u8>> NewWithAllocator<A> for ANSDecoder {
    fn new(_m8: &mut A) -> Self {
        Self::default()
    }
}

impl ANSDecoder {
    fn helper_push_data_rare_cases(&mut self, data: &[u8]) -> usize{
        if self.buffer_a_bytes_required < 16 && self.buffer_a_bytes_required > 4 { // initial setup
            self.sym_count = 0;
            self.state_a = 0;
            self.state_b = 0;
            if data.len() >= 16 {
                self.state_a = u64::from(data[0])|(u64::from(data[1]) << 8)|(u64::from(data[2]) << 16) | (u64::from(data[3]) << 24) |
                    (u64::from(data[4]) << 32)|(u64::from(data[5]) << 40)|(u64::from(data[6]) << 48) | (u64::from(data[7]) << 56);
                self.state_b = u64::from(data[8])|(u64::from(data[9]) << 8)|(u64::from(data[10]) << 16) | (u64::from(data[11]) << 24) |
                    (u64::from(data[12]) << 32)|(u64::from(data[13]) << 40)|(u64::from(data[14]) << 48) | (u64::from(data[15]) << 56);
                self.buffer_a_bytes_required = 0;
                //perror!("Full load buffer_a {} buffer_b {}\n", self.state_a, self.state_b);
                return 16;
            } else {
                self.buffer_a_bytes_required = 16;
            }
        }
        self.helper_push_data_really_rare_cases(data)
    }
    #[cold] // this shouldn't happen unless our caller is really unfriendly and passes us < 64bit aligned buffer sizes
    fn helper_push_data_really_rare_cases(&mut self, data: &[u8]) -> usize{
        if self.buffer_a_bytes_required <= 4 {
            if self.buffer_a_bytes_required == 1 && !data.is_empty() {
                self.state_a <<= 32;
            }
            let bytes_to_copy = cmp::min(data.len(), 5 - self.buffer_a_bytes_required as usize);
            for i in 0..bytes_to_copy {
                self.state_a |= u64::from(data[i]) << ((self.buffer_a_bytes_required - 1 + i as u8) << 3);
            }
            self.buffer_a_bytes_required += bytes_to_copy as u8;
            if self.buffer_a_bytes_required == 5 { // end case: we've made it from 1 to 4
                self.buffer_a_bytes_required = 0;
            }
            return bytes_to_copy;
        }
        assert!(self.buffer_a_bytes_required >= 16);
        let bytes_to_copy = cmp::min(data.len(), 32 - self.buffer_a_bytes_required as usize);
        for i in 0..bytes_to_copy {
            let shift = (self.buffer_a_bytes_required - 16 + i as u8) << 3;
            if shift < 64 {
                self.state_a |= u64::from(data[i]) << shift;
            } else {
                self.state_b |= u64::from(data[i]) << (shift - 64);
            }
        }
        self.buffer_a_bytes_required += bytes_to_copy as u8;
        if self.buffer_a_bytes_required == 32 {
           self.buffer_a_bytes_required = 0; // done with copy 
        }
        return bytes_to_copy;
    }
    fn helper_get_cdf_value_of_sym(&mut self) -> StartFreqType {
        debug_assert!(self.buffer_a_bytes_required == 0);
        return (self.state_a & SCALE_MASK) as i16;
    }
    fn helper_advance_sym(&mut self, start: StartFreqType, freq: StartFreqType) {
        //perror!("inn:{:?} {} {}", self, start, freq);
        let x = (freq as u64) * (self.state_a >> LOG2_SCALE) + (self.state_a & SCALE_MASK) - start as u64;
        //perror!("decode_proc:x = {} x1 = {} bs = {} ls = {} xmax = {} r = {} x1 = {} x1%ls = {} bs+x1%ls = {} start = {}", self.state_a, x, start, freq, (freq as u64) * (self.state_a >> LOG2_SCALE), self.state_a, x, (self.state_a & SCALE_MASK), (freq as u64) * (self.state_a >> LOG2_SCALE) + (self.state_a & SCALE_MASK), start);
        
        self.buffer_a_bytes_required = self.buffer_b_bytes_required;
        // if we've run out of symbols to decode, we don't care what buffer_a's value is, we just clear state and start fresh
        self.buffer_a_bytes_required |= ((u64::from(self.sym_count) == u64::from(NUM_SYMBOLS_BEFORE_FLUSH - 1)) as u8) << 3;
        self.sym_count = self.sym_count.wrapping_add(1);
        // if we ran out of data in our state, we setup buffer_b to require pull from our wordstream
        self.buffer_b_bytes_required = (x < NORMALIZATION_INTERVAL) as u8; // mark to need 4 bytes to continue
        self.state_a = self.state_b;
        self.state_b = x;
        //perror!("out:{:?}, {} {}", self, start, freq);
    }
    fn get_nibble_internal<CDF:BaseCDF>(&mut self, cdf:CDF) -> u8 {
        let cdf_offset = self.helper_get_cdf_value_of_sym();
        let sym_start_freq = cdf.cdf_offset_to_sym_start_and_freq(cdf_offset, LOG2_SCALE);
        self.helper_advance_sym(sym_start_freq.start,
                                sym_start_freq.freq);
        sym_start_freq.sym
    }
    /*
    fn get_bit_from_cdf(&mut self, cdf:CDF2) -> bool {
        let cdf_offset = self.helper_get_cdf_value_of_sym();
        let sym_start_freq = cdf.cdf_offset_to_sym_start_and_freq(cdf_offset, LOG2_SCALE);
        self.helper_advance_sym(sym_start_freq.start,
                                sym_start_freq.freq);
        sym_start_freq.sym != 0
    }*/
}

pub struct ANSEncoder<AllocU8:Allocator<u8>> {
    q: ByteStack<AllocU8>,
    start_freq: ByteStack<AllocU8>,
}
impl<A: Allocator<u8>> NewWithAllocator<A> for ANSEncoder<A> {
    fn new(m8: &mut A) -> Self {
        let q = ByteStack::<A>::new(m8);
        let p = ByteStack::<A>::new(m8);
        assert!(p.stack_bytes_avail() == (NUM_SYMBOLS_BEFORE_FLUSH << 2) as usize);
        ANSEncoder{q:q, start_freq:p}
    }
}

impl<AllocU8:Allocator<u8> > ANSEncoder<AllocU8> {
    fn put_nibble_internal<CDF:CDF16>(&mut self, sym: u8, cdf:CDF) {
        let start_freq = cdf.sym_to_start_and_freq(sym, LOG2_SCALE);
        if !(start_freq.start >= 0 && i32::from(start_freq.start) < (1 << LOG2_SCALE)) {
            debug_assert!(start_freq.start >= 0 && i32::from(start_freq.start) < (1 << LOG2_SCALE));
        }
        debug_assert!(start_freq.start >= 0 && i32::from(start_freq.start) < (1 << LOG2_SCALE));
        debug_assert!(start_freq.freq > 0 && i32::from(start_freq.freq) < (1 << LOG2_SCALE));
        self.put_start_freq(start_freq.start, start_freq.freq);
    }
    fn put_start_freq(&mut self, start: StartFreqType, freq: StartFreqType) {
        debug_assert!(freq != 0);
        // broken if put is called without the queue being empty
        debug_assert!(self.q.is_empty());
        assert!(mem::size_of::<StartFreqType>() == mem::size_of::<u16>()); // so we can use stack_u16 helper
        self.start_freq.stack_u16(freq as u16);
        self.start_freq.stack_u16(start as u16);
        //perror!("Putting {}, {}\n",  start, freq);
        if self.start_freq.bytes().len() == ((NUM_SYMBOLS_BEFORE_FLUSH as usize) << 2) {
            //perror!("Flushing at {}\n",  self.start_freq.bytes().len());
            self.flush_chunk()
        }
    }
    fn reverse_put_sym(&mut self,
            state_a: &mut ANSState,
            state_b: &mut ANSState,
            start: Prob,
            freq: Prob) {
        debug_assert!(start >= 0);
        debug_assert!(freq > 0);
        //perror!("inn:[{}, {}] {} {}", state_a, state_b, start, freq);
        let rescale_lim = ((NORMALIZATION_INTERVAL >> LOG2_SCALE) << 32) * (freq as u64);
        let mut state = *state_a;
        if state >= rescale_lim {
            let state_lower:[u8; 4] = [
                (state & 0xff) as u8,
                ((state >> 8) & 0xff) as u8,
                ((state >> 16) & 0xff) as u8,
                ((state >> 24) & 0xff) as u8,
            ];
            //perror!("rpush {:?}\n", be_state_lower);
            self.q.stack_data(&state_lower[..]);
            state >>= 32;
            debug_assert!(state < rescale_lim);
        }
        let xstate_a = ((state / freq as u64) << LOG2_SCALE) + (state % freq as u64) + start as u64;
        //perror!("encode_proc: x = {} x1 = {} bs = {} ls = {} xmax = {} r = {} x1 = {} x1%ls = {} bs+x1%ls = {} x1/ls<<BITS = {}", *state_a, state, start, freq, rescale_lim, xstate_a, state, state%(freq as u64), (start as u64).wrapping_add(state % (freq as u64)), ((state / freq as u64)<<LOG2_SCALE)); // x1/ls << BITS
        *state_a = *state_b;
        *state_b = xstate_a;
        //perror!("out:[{} {}] {} {}", state_a, state_b, start, freq);
    }
            
    fn flush_chunk(&mut self) {
        let mut len = self.start_freq.bytes().len();
        assert_eq!(len & 3, 0);
        len >>= 2;
        assert!(len <= NUM_SYMBOLS_BEFORE_FLUSH as usize);
        let mut index = 0;
        let mut state_a = ENC_START_STATE;
        let mut state_b = ENC_START_STATE;
        while index < len {
            let start: Prob;
            let freq: Prob;
            {
                let start_freq = self.start_freq.bytes();
                start = Prob::from(start_freq[index * 4]) | (Prob::from(start_freq[index* 4 + 1]) << 8);
                freq = Prob::from(start_freq[index * 4 +2]) | (Prob::from(start_freq[index* 4 + 3]) << 8);
                //perror!("frepush {} {}\n",  start, freq);
            }
            self.reverse_put_sym(&mut state_a, &mut state_b, start, freq);
            index += 1;
        }
        //if (len & 1) == 0 { // odd number of symbols, flip state_a and state_b
            mem::swap(&mut state_a, &mut state_b);
        //}
        let state_ab:[u8;16] = [
            (state_a & 0xff) as u8,
            ((state_a >> 8) & 0xff) as u8,
            ((state_a >> 16) & 0xff) as u8,
            ((state_a >> 24) & 0xff) as u8,
            ((state_a >> 32) & 0xff) as u8,
            ((state_a >> 40) & 0xff) as u8,
            ((state_a >> 48) & 0xff) as u8,
            ((state_a >> 56) & 0xff) as u8,
            (state_b & 0xff) as u8,
            ((state_b >> 8) & 0xff) as u8,
            ((state_b >> 16) & 0xff) as u8,
            ((state_b >> 24) & 0xff) as u8,
            ((state_b >> 32) & 0xff) as u8,
            ((state_b >> 40) & 0xff) as u8,
            ((state_b >> 48) & 0xff) as u8,
            ((state_b >> 56) & 0xff) as u8,
        ];
        //perror!("efinal: [{} {}]", state_a, state_b);
        self.q.stack_data(&state_ab[..]);
        self.start_freq.reset();
    }
}

impl<AllocU8: Allocator<u8>> EntropyEncoder for ANSEncoder<AllocU8> {
    type Queue = ByteStack<AllocU8>;
    fn get_internal_buffer(&mut self) -> &mut Self::Queue {
        &mut self.q
    }
    fn put_bit(&mut self, bit: bool, mut prob_of_false: u8) {
        if prob_of_false == 0 {
            prob_of_false = 1;
        }
        self.put_start_freq(if bit {((Prob::from(prob_of_false))) << (LOG2_SCALE - 8) } else {0},
                            (if bit {256 - (Prob::from(prob_of_false))} else {Prob::from(prob_of_false)}) << (LOG2_SCALE - 8))
    }
    fn put_nibble<CDF:CDF16>(&mut self, symbol: u8, cdf:&CDF) {
        self.put_nibble_internal(symbol, *cdf)
    }

    fn flush(&mut self) {
        self.flush_chunk()
    }
}
impl ByteQueue for ANSDecoder {
    fn num_push_bytes_avail(&self) -> usize {
        if self.buffer_a_bytes_required == 0 {
            return 0;
        }
        if self.buffer_a_bytes_required == 1 {
            return 4
        }
        if self.buffer_a_bytes_required <= 5 {
            return 5 - self.buffer_a_bytes_required as usize;
        }
        if self.buffer_a_bytes_required >= 16 {
            return 32 - self.buffer_a_bytes_required as usize;
        }
        return 16
    }
    fn num_pop_bytes_avail(&self) -> usize {
        0
    }
    fn push_data(&mut self, data:&[u8]) -> usize {
        if self.buffer_a_bytes_required == 0 {
            return 0;
        }
        if self.buffer_a_bytes_required == 1 {
            if data.len() >= 4 {
                self.state_a <<= 32;
                let old_state_a = self.state_a;
                self.state_a |= u64::from(data[0])|(u64::from(data[1]) << 8)|(u64::from(data[2]) << 16) | (u64::from(data[3]) << 24);
                let new_state_a = self.state_a;
                assert!(new_state_a >= old_state_a);
                self.buffer_a_bytes_required = 0;
                return 4;
            }
        }
        self.helper_push_data_rare_cases(data)
    }
    fn pop_data(&mut self, _data:&mut [u8]) -> usize {
        assert!(false);
        0
    }    
}
impl EntropyDecoder for ANSDecoder {
    type Queue = Self;
    fn get_internal_buffer(&mut self) -> &mut Self::Queue {
        self
    }
    fn get_nibble<CDF:CDF16>(&mut self, cdf:&CDF) -> u8 {
        self.get_nibble_internal(*cdf)
    }
    fn get_bit(&mut self, mut prob_of_false: u8) -> bool {
        if prob_of_false ==0 {
            prob_of_false =1;
        }
        let cdf_offset = self.helper_get_cdf_value_of_sym();
        let rescaled_prob_of_false = ((Prob::from(prob_of_false))) << (LOG2_SCALE - 8);
        let inv_rescaled_prob_of_false = ((256 - Prob::from(prob_of_false))) << (LOG2_SCALE - 8);
        let bit = cdf_offset >= rescaled_prob_of_false;
        self.helper_advance_sym(if bit {rescaled_prob_of_false} else {0},
                                if bit {inv_rescaled_prob_of_false} else {rescaled_prob_of_false});
        bit
    }
    fn flush(&mut self) -> BrotliResult {
        BrotliResult::ResultSuccess
    }
}

impl<AllocU8: Allocator<u8>> ArithmeticEncoderOrDecoder for ANSEncoder<AllocU8> {
    arithmetic_encoder_or_decoder_methods!();
}

impl BillingCapability for ANSDecoder {
}

#[cfg(test)]
mod test {
    extern crate std;
    #[allow(unused_imports)]
    use std::io::Write;
    
    use core;
    use super::{
        ANSDecoder,
        ANSEncoder,
    };
    use encoder::{
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
    use super::super::test_helper::HeapAllocator;
    const BITS: u8 = 8;
    
    fn init_src(src: &mut [u8]) -> u8 {
        let mut ones = 0u64;
        let seed: [u8; 16] = [0xef, 0xbf,0xff,0xfd,0xef,0x3f,0xc0,0xfd,0xef,0xc0,0xff,0xfd,0xdf,0x3f,0xff,0xfd];
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
    fn entropy_single_trait_bench(b: &mut Bencher) {
        const SZ: usize = 1024*1024;
        let mut m8 = HeapAllocator::<u8>{default_value: 0u8};
        let mut src = m8.alloc_cell(SZ);
        let mut dst = m8.alloc_cell(SZ);
        let mut start = m8.alloc_cell(SZ);
        let mut end = m8.alloc_cell(SZ);
        let (prob0, _optimal) = setup_test_return_optimal(
            src.slice_mut(), dst.slice_mut(), end.slice_mut(), start.slice_mut());
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
    fn entropy16_decode_bench(b: &mut Bencher) {
        const SZ: usize = 1024*1024;
        let mut m8 = HeapAllocator::<u8>{default_value: 0u8};
        let mut src = m8.alloc_cell(SZ);
        let mut dst = m8.alloc_cell(SZ);
        let mut end = m8.alloc_cell(SZ);
        let mut start = m8.alloc_cell(SZ);
        let (prob0, _optimal) = setup_test_return_optimal(
            src.slice_mut(), dst.slice_mut(), end.slice_mut(), start.slice_mut());
        let mut e = ANSEncoder::new(&mut m8);
        let mut n: usize = 0;
        encode_test_helper(&mut e, prob0, src.slice(), dst.slice_mut(), &mut n, false);
        perror!("encoded size: {}", n);
        let nbits = n * 8;
        let actual = nbits as f64;
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
    fn setup_test_return_optimal(src:&mut[u8], _dst:&mut[u8], _end:&mut [u8], start:&mut[u8]) -> (u8, f64) {
        let prob = init_src(src);
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
        let (prob0, optimal) = setup_test_return_optimal(src, dst, end, start);
        let mut d = ANSDecoder::new(&mut m8);
        let mut e = ANSEncoder::new(&mut m8);
        let mut n: usize = 0;
        encode_test_helper(&mut e, prob0, src, dst, &mut n, trailing_bit_and_one_byte_at_a_time);
        perror!("encoded size: {}", n);

        let nbits = n * 8;
        let actual = nbits as f64;
        perror!("effeciency: {}", actual / optimal);
        //assert!(actual >= optimal);
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
}

