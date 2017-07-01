use interface::ArithmeticEncoderOrDecoder;
use super::probability::CDF16;
use brotli_decompressor::BrotliResult;





pub struct BillingArithmeticCoder<Coder:ArithmeticEncoderOrDecoder>(Coder);

impl<Coder:ArithmeticEncoderOrDecoder+Default> Default for BillingArithmeticCoder<Coder> {
   fn default() -> Self {
       BillingArithmeticCoder::<Coder>(Coder::default())
   }
}

impl<Coder:ArithmeticEncoderOrDecoder> ArithmeticEncoderOrDecoder for BillingArithmeticCoder<Coder> {
    fn drain_or_fill_internal_buffer(&mut self,
                                     input_buffer:&[u8],
                                     input_offset:&mut usize,
                                     output_buffer:&mut [u8],
                                     output_offset: &mut usize) -> BrotliResult{
       self.0.drain_or_fill_internal_buffer(input_buffer, input_offset, output_buffer, output_offset)
    }
    fn get_or_put_bit(&mut self,
                      bit: &mut bool,
                      prob_of_false: u8) {
       self.0.get_or_put_bit(bit, prob_of_false)
    }
    fn get_or_put_nibble<C: CDF16>(&mut self,
                                   nibble: &mut u8,
                                   prob: &C){
       self.0.get_or_put_nibble(nibble, prob)
    }
    fn close(&mut self) -> BrotliResult {
        self.0.close()
    }
}

