#![allow(unused_macros)]
use interface::ArithmeticEncoderOrDecoder;
use super::probability::CDF16;
use brotli_decompressor::BrotliResult;


macro_rules! println_stderr(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);



pub struct BillingArithmeticCoder<Coder:ArithmeticEncoderOrDecoder> {
    coder: Coder,
    vbit_count: f64,
    bit_count: f64,
}


#[cfg(feature="billing")]
impl<Coder:ArithmeticEncoderOrDecoder+Default> BillingArithmeticCoder<Coder> {
    pub fn print_compression_ratio(&self, original_bytes : usize) {
         println_stderr!("{:.2}/{:}  Ratio {:.3}%",
             self.bit_count / 8.0, original_bytes, self.bit_count * 100.0 / 8.0 / (original_bytes as f64));
    }
}

impl<Coder:ArithmeticEncoderOrDecoder+Default> Default for BillingArithmeticCoder<Coder> {
   fn default() -> Self {
       BillingArithmeticCoder::<Coder>{
           coder:Coder::default(),
           bit_count:0.0,
           vbit_count:0.0,
       }
   }
}
#[cfg(feature="billing")]
use std::io::Write;
#[cfg(feature="billing")]
impl<Coder:ArithmeticEncoderOrDecoder> Drop for BillingArithmeticCoder<Coder> {
  fn drop(&mut self) {
     println_stderr!("Bit count {:.1} Byte Count: {:.3}\nNumber of virtual bits serialized = {:.0}",
         self.bit_count, self.bit_count / 8.0, self.vbit_count);
  }
}
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
       self.coder.get_or_put_bit(bit, prob_of_false);
       let mut actual_prob = (prob_of_false as f64 + 0.5) / 256.0;
       if *bit {
           actual_prob = 1.0 - actual_prob;
       }
       self.bit_count -= actual_prob.log2();
       self.vbit_count += 1.0;
    }
    fn get_or_put_nibble<C: CDF16>(&mut self,
                                   nibble: &mut u8,
                                   prob: &C){
       self.coder.get_or_put_nibble(nibble, prob);
       let actual_prob = prob.pdf(*nibble) as f64 / (prob.max() as f64);
       self.bit_count -= actual_prob.log2();
       self.vbit_count += 4.0;
    }
    fn close(&mut self) -> BrotliResult {
        self.coder.close()
    }
}

