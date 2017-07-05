#![allow(unknown_lints,unused_macros,unused_imports)]
use interface::{ArithmeticEncoderOrDecoder, BillingDesignation};
use super::probability::CDF16;
use brotli_decompressor::BrotliResult;

#[cfg(feature="billing")]
mod billing {
    pub use std::collections::HashMap;
    pub use std::string::String;
}

#[cfg(feature="billing")]
pub use std::io::Write;

macro_rules! println_stderr(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);

#[cfg(feature="billing")]
pub struct BillingArithmeticCoder<Coder:ArithmeticEncoderOrDecoder> {
    coder: Coder,
    counter: billing::HashMap<BillingDesignation, (f64, f64)>
}

#[cfg(feature="billing")]
impl<Coder:ArithmeticEncoderOrDecoder+Default> Default for BillingArithmeticCoder<Coder> {
   fn default() -> Self {
       BillingArithmeticCoder::<Coder>{
           coder: Coder::default(),
           counter: billing::HashMap::new(),
       }
   }
}

#[cfg(feature="billing")]
impl<Coder:ArithmeticEncoderOrDecoder+Default> BillingArithmeticCoder<Coder> {
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
impl<Coder:ArithmeticEncoderOrDecoder> Drop for BillingArithmeticCoder<Coder> {
    fn drop(&mut self) {
        let max_keystr_len = self.counter.iter().map(|(k, _)| format!("{:?}", k).len()).max().unwrap_or(5);
        let report = |k: billing::String, v: (f64, f64)| {
            let keystr = format!("{1:0$}", max_keystr_len, k);
            println_stderr!("{} Bit count: {:9.1} Byte count: {:11.3} Virtual bits: {:7.0}",
                            keystr, v.0, v.0 / 8.0, v.1);
        };
        let mut total_bits : f64 = 0.0;
        let mut total_vbits : f64 = 0.0;
        for (k, v) in self.counter.iter() {
            report(format!("{:?}", k), *v);
            total_bits += v.0;
            total_vbits += v.1;
        }
        report(billing::String::from("Total"), (total_bits, total_vbits));
    }
}

#[cfg(feature="billing")]
impl<Coder:ArithmeticEncoderOrDecoder> ArithmeticEncoderOrDecoder for BillingArithmeticCoder<Coder> {
    fn drain_or_fill_internal_buffer(&mut self,
                                     input_buffer:&[u8],
                                     input_offset:&mut usize,
                                     output_buffer:&mut [u8],
                                     output_offset: &mut usize) -> BrotliResult{
       self.coder.drain_or_fill_internal_buffer(input_buffer, input_offset, output_buffer, output_offset)
    }
    fn get_or_put_bit(&mut self,
                      bit: &mut bool,
                      prob_of_false: u8) {
        self.get_or_put_bit_with_billing(bit, prob_of_false, BillingDesignation::Unknown)
    }
    fn get_or_put_bit_with_billing(&mut self,
                                   bit: &mut bool,
                                   prob_of_false: u8,
                                   billing: BillingDesignation) {
        self.coder.get_or_put_bit(bit, prob_of_false);
        let mut actual_prob = (prob_of_false as f64 + 0.5) / 256.0;
        if *bit {
            actual_prob = 1.0 - actual_prob;
        }
        let v = self.counter.entry(billing).or_insert((0.0, 0.0));
        (*v).0 += -actual_prob.log2();
        (*v).1 += 1.0;
    }
    fn get_or_put_nibble<C: CDF16>(&mut self,
                                   nibble: &mut u8,
                                   prob: &C) {
        self.get_or_put_nibble_with_billing(nibble, prob, BillingDesignation::Unknown)
    }
    fn get_or_put_nibble_with_billing<C: CDF16>(&mut self,
                                                nibble: &mut u8,
                                                prob: &C,
                                                billing: BillingDesignation) {
        self.coder.get_or_put_nibble(nibble, prob);
        let actual_prob = prob.pdf(*nibble) as f64 / (prob.max() as f64);
        let v = self.counter.entry(billing).or_insert((0.0, 0.0));
        (*v).0 += -actual_prob.log2();
        (*v).1 += 4.0;
    }
    fn close(&mut self) -> BrotliResult {
        self.coder.close()
    }
}
