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
use probability::{CDF16, BaseCDF, Prob, LOG2_SCALE, ProbRange};
use super::interface::{
    ArithmeticEncoderOrDecoder,
    NewWithAllocator,
    BillingCapability,
};
use super::DivansResult;
use super::arithmetic_coder::{
    EntropyEncoder,
    EntropyDecoder,
    ByteQueue,
};

/*
#[cfg(test)]
#[cfg(not(feature="benchmark"))]
macro_rules! perror(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);

#[cfg(not(test))]
macro_rules! perror(
    ($($val:tt)*) => { {
    } }
);
#[cfg(test)]
#[cfg(feature="benchmark")]
macro_rules! perror(
    ($($val:tt)*) => { {
    } }
);
*/
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
    fn free(&mut self, m8: &mut A) {
        m8.free_cell(core::mem::replace(&mut self.data, A::AllocatedMemory::default()));
    }
}

impl<AllocU8: Allocator<u8>> ByteStack<AllocU8> {
    pub fn mov(&mut self) -> Self {
        ByteStack::<AllocU8> {
            data:core::mem::replace(&mut self.data, AllocU8::AllocatedMemory::default()),
            nbytes:self.nbytes,
        }
    }
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

const NUM_SYMBOLS_BEFORE_FLUSH:u32 = (MAX_BUFFER_SIZE as u32) >> 2;
const SCALE_MASK:u64 = ((1u64 << LOG2_SCALE) - 1);

#[derive(Debug, Clone)]
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
    fn free(&mut self, _m8: &mut A){
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
    #[inline(always)]
    fn helper_get_cdf_value_of_sym(&mut self) -> StartFreqType {
        debug_assert!(self.buffer_a_bytes_required == 0);
        return (self.state_a & SCALE_MASK) as i16;
    }
    #[inline(always)]
    fn helper_advance_sym(&mut self, start: StartFreqType, freq: StartFreqType) {
        //perror!("inn:{:?} {} {}", self, start, freq);
        //perror!("decode_proc:x = {} x1 = {} bs = {} ls = {} xmax = {} r = {} x1 = {} x1%ls = {} bs+x1%ls = {} start = {}", self.state_a, x, start, freq, (freq as u64) * (self.state_a >> LOG2_SCALE), self.state_a, x, (self.state_a & SCALE_MASK), (freq as u64) * (self.state_a >> LOG2_SCALE) + (self.state_a & SCALE_MASK), start);
        
        self.buffer_a_bytes_required = self.buffer_b_bytes_required;
        // if we've run out of symbols to decode, we don't care what buffer_a's value is, we just clear state and start fresh
        self.buffer_a_bytes_required |= ((u64::from(self.sym_count) == u64::from(NUM_SYMBOLS_BEFORE_FLUSH - 1)) as u8) << 3;
        let x = (freq as u64) * (self.state_a >> LOG2_SCALE) + (self.state_a & SCALE_MASK) - start as u64;
        self.sym_count = self.sym_count.wrapping_add(1);
        // if we ran out of data in our state, we setup buffer_b to require pull from our wordstream
        self.buffer_b_bytes_required = (x < NORMALIZATION_INTERVAL) as u8; // mark to need 4 bytes to continue
        self.state_a = self.state_b;
        self.state_b = x;
        //perror!("out:{:?}, {} {}", self, start, freq);
    }
    #[inline(always)]
    fn get_nibble_internal<CDF:BaseCDF>(&mut self, cdf:CDF) -> (u8, ProbRange) {
        let cdf_offset = self.helper_get_cdf_value_of_sym();
        let sym_start_freq = cdf.cdf_offset_to_sym_start_and_freq(cdf_offset);
        self.helper_advance_sym(sym_start_freq.range.start,
                                sym_start_freq.range.freq);
        (sym_start_freq.sym, sym_start_freq.range)
    }
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
    fn free(&mut self, m8: &mut A) {
        self.q.free(m8);
        self.start_freq.free(m8);
    }
}

impl<AllocU8:Allocator<u8> > ANSEncoder<AllocU8> {
    fn mov_internal(&mut self) -> Self {
        let old_q = self.q.mov();
        ANSEncoder::<AllocU8> {
            q:old_q,
            start_freq:self.start_freq.mov(),
        }
    }
    fn put_nibble_internal<CDF:CDF16>(&mut self, sym: u8, cdf:CDF) -> ProbRange {
        let start_freq = cdf.sym_to_start_and_freq(sym).range;
        if !(start_freq.start >= 0 && i32::from(start_freq.start) < (1 << LOG2_SCALE)) {
            debug_assert!(start_freq.start >= 0 && i32::from(start_freq.start) < (1 << LOG2_SCALE));
        }
        debug_assert!(start_freq.start >= 0 && i32::from(start_freq.start) < (1 << LOG2_SCALE));
        debug_assert!(start_freq.freq > 0 && i32::from(start_freq.freq) < (1 << LOG2_SCALE));
        self.put_start_freq(start_freq.start, start_freq.freq);
        start_freq
    }
    fn put_start_freq(&mut self, start: StartFreqType, freq: StartFreqType) {
        debug_assert!(freq != 0);
        // broken if put is called without the queue being empty
        debug_assert!(self.q.is_empty());
        assert!(mem::size_of::<StartFreqType>() == mem::size_of::<u16>()); // so we can use stack_u16 helper
        self.start_freq.stack_u16(freq as u16);
        self.start_freq.stack_u16(start as u16);

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
        if len == 0 {
            return;
        }
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
    #[inline(always)]
    fn get_internal_buffer_mut(&mut self) -> &mut Self::Queue {
        &mut self.q
    }
    #[inline(always)]
    fn get_internal_buffer(&self) -> &Self::Queue {
        &self.q
    }
    fn put_bit(&mut self, bit: bool, mut prob_of_false: u8) {
        if prob_of_false == 0 {
            prob_of_false = 1;
        }
        self.put_start_freq(if bit {Prob::from(prob_of_false) << (LOG2_SCALE - 8) } else {0},
                            if bit {256 - Prob::from(prob_of_false)} else {Prob::from(prob_of_false)} << (LOG2_SCALE - 8))
    }
    fn put_nibble<CDF:CDF16>(&mut self, symbol: u8, cdf:&CDF) -> ProbRange {
        self.put_nibble_internal(symbol, *cdf)
    }

    fn flush(&mut self) {
        self.flush_chunk()
    }
}
impl ByteQueue for ANSDecoder {
    #[inline(always)]
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
        16
    }
    #[inline(always)]
    fn num_pop_bytes_avail(&self) -> usize {
        0
    }
    #[inline(always)]
    fn push_data(&mut self, data:&[u8]) -> usize {
        if self.buffer_a_bytes_required == 0 {
            return 0;
        }
        if self.buffer_a_bytes_required == 1 && data.len() >= 4 {
            self.state_a <<= 32;
            let old_state_a = self.state_a;
            self.state_a |= u64::from(data[0])|(u64::from(data[1]) << 8)|(u64::from(data[2]) << 16) | (u64::from(data[3]) << 24);
            let new_state_a = self.state_a;
            assert!(new_state_a >= old_state_a);
            self.buffer_a_bytes_required = 0;
            return 4;
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
    #[inline(always)]
    fn get_internal_buffer_mut(&mut self) -> &mut Self::Queue {
        self
    }
    #[inline(always)]
    fn get_internal_buffer(&self) -> &Self::Queue {
        self
    }
    #[inline(always)]
    fn get_nibble<CDF:CDF16>(&mut self, cdf:&CDF) -> (u8, ProbRange) {
        self.get_nibble_internal(*cdf)
    }
    fn get_bit(&mut self, mut prob_of_false: u8) -> bool {
        if prob_of_false ==0 {
            prob_of_false =1;
        }
        let cdf_offset = self.helper_get_cdf_value_of_sym();
        let rescaled_prob_of_false = Prob::from(prob_of_false) << (LOG2_SCALE - 8);
        let inv_rescaled_prob_of_false = (256 - Prob::from(prob_of_false)) << (LOG2_SCALE - 8);
        let bit = cdf_offset >= rescaled_prob_of_false;
        self.helper_advance_sym(if bit {rescaled_prob_of_false} else {0},
                                if bit {inv_rescaled_prob_of_false} else {rescaled_prob_of_false});
        bit
    }
    fn flush(&mut self) -> DivansResult {
        DivansResult::Success
    }
}

impl<AllocU8: Allocator<u8>> ArithmeticEncoderOrDecoder for ANSEncoder<AllocU8> {
    arithmetic_encoder_or_decoder_methods!();
}

impl BillingCapability for ANSDecoder {
}

