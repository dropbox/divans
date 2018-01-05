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

#![allow(unknown_lints,unused_macros,unused_imports)]
use core::iter::FromIterator;
use core::marker::PhantomData;
use alloc::{Allocator};
use interface::{ArithmeticEncoderOrDecoder, BillingDesignation, NewWithAllocator, BillingCapability};
use super::probability::{CDF16, ProbRange};
use brotli::BrotliResult;

#[cfg(feature="billing")]
mod billing {
    pub use std::collections::HashMap;
    pub use std::string::String;
    pub use std::vec::Vec;
}

#[cfg(feature="billing")]
pub use std::io::Write;

macro_rules! println_stderr(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);

#[cfg(feature="billing")]
pub struct BillingArithmeticCoder<AllocU8:Allocator<u8>, Coder:ArithmeticEncoderOrDecoder> {
    coder: Coder,
    counter: billing::HashMap<BillingDesignation, (f64, f64)>,
    _phantom: PhantomData<AllocU8>,
}

#[cfg(feature="billing")]
impl<AllocU8:Allocator<u8>,
     Coder:ArithmeticEncoderOrDecoder+NewWithAllocator<AllocU8>> NewWithAllocator<AllocU8> for BillingArithmeticCoder<AllocU8, Coder> {
   fn new(m8: &mut AllocU8) -> Self {
       BillingArithmeticCoder::<AllocU8, Coder>{
           coder: Coder::new(m8),
           counter: billing::HashMap::new(),
           _phantom:PhantomData::<AllocU8>::default(),
       }
   }
    fn free(&mut self, m8: &mut AllocU8) {
        self.coder.free(m8);
    }
}

#[cfg(feature="billing")]
impl<AllocU8:Allocator<u8>, Coder:ArithmeticEncoderOrDecoder> BillingArithmeticCoder<AllocU8, Coder> {
    // Return the (bits, virtual bits) pair.
    pub fn get_total(&self) -> (f64, f64) {
        let mut total_bits : f64 = 0.0;
        let mut total_vbits : f64 = 0.0;
        for (_, v) in self.counter.iter() {
            total_bits += v.0;
            total_vbits += v.1;
        }
        (total_bits, total_vbits)
    }
    pub fn print_compression_ratio(&self, original_bytes : usize) {
        let (total_bits, _) = self.get_total();
        println_stderr!("{:.2}/{:}  Ratio {:.3}%",
                        total_bits / 8.0, original_bytes, total_bits * 100.0 / 8.0 / (original_bytes as f64));
    }
}

#[cfg(feature="billing")]
impl<AllocU8:Allocator<u8>, Coder:ArithmeticEncoderOrDecoder> Drop for BillingArithmeticCoder<AllocU8, Coder> {
    fn drop(&mut self) {
        let max_key_len = self.counter.keys().map(|k| format!("{:?}", k).len()).max().unwrap_or(5);
        let report = |k, v: (f64, f64)| {
            println_stderr!("{1:0$} Bit count: {2:9.1} Byte count: {3:11.3} Virtual bits: {4:7.0}",
                            max_key_len, k, v.0, v.0 / 8.0, v.1);
        };
        let mut sorted_entries = billing::Vec::from_iter(self.counter.iter());
        sorted_entries.sort_by_key(|&(k, _)| format!("{:?}", k));

        let mut total_bits : f64 = 0.0;
        let mut total_vbits : f64 = 0.0;

        for (k, v) in sorted_entries {
            report(format!("{:?}", k), *v);
            total_bits += v.0;
            total_vbits += v.1;
        }
        report(billing::String::from("Total"), (total_bits, total_vbits));
    }
}

#[cfg(feature="billing")]
impl<AllocU8:Allocator<u8>, Coder:ArithmeticEncoderOrDecoder> ArithmeticEncoderOrDecoder for BillingArithmeticCoder<AllocU8, Coder> {
    fn drain_or_fill_internal_buffer(&mut self,
                                     input_buffer: &[u8],
                                     input_offset: &mut usize,
                                     output_buffer: &mut [u8],
                                     output_offset: &mut usize) -> BrotliResult{
       self.coder.drain_or_fill_internal_buffer(input_buffer, input_offset, output_buffer, output_offset)
    }
    fn get_or_put_bit_without_billing(&mut self,
                                      bit: &mut bool,
                                      prob_of_false: u8) {
        self.get_or_put_bit(bit, prob_of_false, BillingDesignation::Unknown)
    }
    fn get_or_put_bit(&mut self,
                      bit: &mut bool,
                      prob_of_false: u8,
                      billing: BillingDesignation) {
        self.coder.get_or_put_bit_without_billing(bit, prob_of_false);
        let mut actual_prob = (prob_of_false as f64 + 0.5) / 256.0;
        if *bit {
            actual_prob = 1.0 - actual_prob;
        }
        let v = self.counter.entry(billing).or_insert((0.0, 0.0));
        (*v).0 += -actual_prob.log2();
        (*v).1 += 1.0;
    }
    fn get_or_put_nibble_without_billing<C: CDF16>(&mut self,
                                                   nibble: &mut u8,
                                                   prob: &C) -> ProbRange {
        self.get_or_put_nibble(nibble, prob, BillingDesignation::Unknown)
    }
    fn get_or_put_nibble<C: CDF16>(&mut self,
                                   nibble: &mut u8,
                                   prob: &C,
                                   billing: BillingDesignation) -> ProbRange {
        let ret = self.coder.get_or_put_nibble_without_billing(nibble, prob);
        let actual_prob = prob.pdf(*nibble) as f64 / (prob.max() as f64);
        let v = self.counter.entry(billing).or_insert((0.0, 0.0));
        (*v).0 += -actual_prob.log2();
        (*v).1 += 4.0;
        ret
    }
    fn close(&mut self) -> BrotliResult {
        self.coder.close()
    }
}

// only need to implement this for feature=billing, since it's defined for any T in the default case
#[cfg(feature="billing")]
impl<AllocU8:Allocator<u8>, Coder:ArithmeticEncoderOrDecoder> BillingCapability for BillingArithmeticCoder<AllocU8, Coder> {
    fn debug_print(&self, byte_size: usize) {
        self.print_compression_ratio(byte_size);
    }
}

#[cfg(not(feature="billing"))]
macro_rules! DefaultEncoderType(
    () => {super::ans::ANSEncoder<AllocU8>}
);

#[cfg(not(feature="billing"))]
macro_rules! DefaultDecoderType(
    () => {ans::ANSDecoder}
);


#[cfg(feature="billing")]
macro_rules! DefaultEncoderType(
    () => { super::billing::BillingArithmeticCoder<AllocU8, ::ans::ANSEncoder<AllocU8>> }
);

#[cfg(feature="billing")]
macro_rules! DefaultDecoderType(
    () => { billing::BillingArithmeticCoder<AllocU8, ::ans::ANSDecoder> }
);
