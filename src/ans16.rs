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
use core::marker::PhantomData;
use alloc::{
    Allocator,
    SliceWrapper,
    SliceWrapperMut
};
use core::default::Default;
use probability::CDF16;
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

type ANSState u64;
type StartFreqType i16;
const NORMALIZATION_INTERVAL: ANSState = 1u64 << 31;
const ENC_START_STATE: ANSState = NORMALIZATION_INTERVAL;
const LOG2_SCALE: u32 = 16;
const NUM_SYMBOLS_BEFORE_FLUSH:u32 = 65536;
const SCALE_MASK:u64 = ((1u64 << LOG2_SCALE) - 1);
pub struct ANSDecoder {
    state_a: u64,
    state_b: u64,
    sym_count: u32, // FIXME: this may be able to be a u16
    buffer_a_bytes_required: u8, // needs 8 to start with
    buffer_b_bytes_required: u8, // needs 8 to start with
}

impl<A: Allocator<u8>> NewWithAllocator<A> for ANSDecoder {
    fn new(m8: &mut A) -> Self {
        ANSDecoder{
            state_a: 0,
            state_b: 0,
            sym_count: 0,
            buffer_a_bytes_required: 8,
            buffer_b_bytes_required: 8,
        }
    }
}

impl ANSDecoder {
    fn helper_get_cdf_value_of_sym(&mut self) StartFreqType {
        debug_assert!(self.buffer_a_bytes_required == 0);
        return state_a & SCALE_MASK;
    }
    fn helper_advance_sym(&mut self, start: StartFreqType, freq: StartFreqType) {
        let x = (freq as u64) * (state_a >> LOG2_SCALE) + (state_a & SCALE_MASK) - start as u64;
        self.buffer_a_bytes_required = buffer_b_bytes_required;
        // if we've run out of symbols to decode, we don't care what buffer_a's value is, we just clear state and start fresh
        self.buffer_a_bytes_required |= (u64::from(self.byte_count) + 1 == u64::from(NUM_SYMBOLS_BEFORE_FLUSH)) as u8 << 3;
        self.byte_count += 1;
        // if we ran out of data in our state, we setup buffer_b to require pull from our wordstream
        self.buffer_b_bytes_required = ((x < NORMALIZATION_INTERVAL) as u8) << 2; // need 4 bytes to continue (may want to make this constant 1)
        self.state_a = self.state_b;
        self.state_b = x;
    }
    fn get_nibble(&mut self, cdf:CDF16) -> u8 {
        //FIXME: this is where it gets real and we need to use SSE and things
        
    }
    fn get_bit(&mut self, prob_of_false:Probability) -> bool {
        let cdf_value = self.helper_get_cdf_value_of_sym();
        let rescaled_prob_of_false = (StartFreqType::from(prob_of_false) << (mem::size_of::<Probability>() - LOG2_SCALE));
        let bit = cdf_value > rescaled_prob_of_false;
        self.helper_advance_sym(if bit {rescaled_prob_of_false} else {0},
                                if bit {(u32::from(1) << LOG2_SCALE) - u32::from(rescaled_prob_of_false) as StartFreqType} else {rescaled_prob_of_false});
        bit
    }
}

pub struct ANSEncoder<AllocU8:Allocator<u8>> {
    q: ByteStack<AllocU8>,
    start_freq: ByteStack<AllocU8>,
}

impl<AllocU8:Allocator<u8> > ANSEncoder<AllocU8> {
    fn put_sym(&mut self, start: StartFreqType, freq: StartFreqType) {
        debug_assert!(freq != 0);
        // broken if put is called without the queue being empty
        debug_assert!(self.q.is_empty());
        assert!(mem::size_of::<StartFreqType>() == mem::size_of::<u16>()); // so we can use stack_u16 helper
        self.start_freq.stack_u16(freq);
        self.start_freq.stack_u16(start);
        if self.start_freq.bytes().len() == NUM_SYMBOLS_BEFORE_FLUSH * 4 {
            self.flush()
        }
    }
    fn reverse_put_sym(&mut self,
            state_a: &mut ANSState,
            state_b: &mut ANSState,
            start: u16,
            freq: u16) {
        debug_assert!(freq != 0);
        let rescale_lim = ((NORMALIZATION_INTERVAL >> LOG2_SCALE) << 32) * freq;
        let state = *state_a;
        if state >= rescale_lim {
            let state_lower[u8; 4] = [
                (state & 0xff) as u8,
                ((state >> 8) & 0xff) as u8,
                ((state >> 16) & 0xff) as u8,
                ((state >> 24) & 0xff) as u8,
            ];
            self.q.stack_data(&state_lower[..]);
            state >>= 32;
            debug_assert!(state < rescale_lim);
        }
        *state_a = *state_b;
        *state_b = ((state / freq) << LOG2_SCALE) + (state % freq) + start;
    }
            
    fn flush(&mut self) {
        let start_freq = self.start_freq.bytes();
        let len = start_freq.len();
        assert_eq!(len & 3, 0);
        len >>= 2;
        assert_eq!(len <= NUM_SYMBOLS_BEFORE_FLUSH);
        let mut index = 0;
        let mut state_a = ENC_START_STATE;
        let mut state_b = ENC_START_STATE;
        while index < len {
            let start = u16::from(start_freq[index * 4]) + u16::from(start_freq[index* 4 + 1]);
            let freq = u16::from(start_freq[index * 4 +2]) + u16::from(start_freq[index* 4 + 3]);
            self.reverse_put_sym(&mut stateA, &mut stateB, start, freq);
            index += 1;
        }
        if (len & 1) != 0 { // odd number of symbols, flip state_a and state_b
            (state_a, state_b) = (state_b, state_a);
        }
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
        self.q.stack_data(&state_ab[..]);
        self.start_freq.reset();
    }
}

