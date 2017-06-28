use super::encoder::{
    EntropyEncoder,
    ByteQueue,
    RegisterQueue,
    EntropyDecoder,
};
use probability::{CDF16, CDFUpdater};
use super::interface::ArithmeticEncoderOrDecoder;
use super::BrotliResult;
#[derive(Default)]
pub struct DebugEncoder {
    buffer: RegisterQueue,
}


impl EntropyEncoder for DebugEncoder {
    type Queue = RegisterQueue;
    fn get_internal_buffer(&mut self) -> &mut RegisterQueue {
        &mut self.buffer
    }
    fn put_bit(&mut self, bit: bool, prob_of_false: u8) {
        assert!(self.buffer.num_push_bytes_avail() > 0);
        let buf_to_push = [prob_of_false ^ bit as u8];
        let cnt = self.buffer.push_data(&buf_to_push[..]);
        assert_eq!(cnt, 1);
    }
    fn flush(&mut self) {

    }
}

#[derive(Default)]
pub struct DebugDecoder {
    buffer: RegisterQueue,
}


impl EntropyDecoder for DebugDecoder {
    type Queue = RegisterQueue;
    fn get_internal_buffer(&mut self) -> &mut RegisterQueue {
        &mut self.buffer
    }
    fn get_bit(&mut self, prob_of_false: u8) -> bool {
        assert!(self.buffer.num_pop_bytes_avail() > 0);
        let mut buf_to_pop = [0u8];
        let cnt = self.buffer.pop_data(&mut buf_to_pop[..]);
        assert_eq!(cnt, 1);
        let return_value = buf_to_pop[0] ^ prob_of_false;
        if return_value != 0 {
            assert_eq!(return_value, 1);
        }
        return_value != 0
    }
    fn flush(&mut self) -> BrotliResult {
        return BrotliResult::ResultSuccess;
    }
}


impl ArithmeticEncoderOrDecoder for DebugEncoder {
    fn drain_or_fill_internal_buffer(&mut self,
                                     input_buffer:&[u8],
                                     input_offset:&mut usize,
                                     output_buffer:&mut [u8],
                                     output_offset: &mut usize) -> BrotliResult {
        let mut ibuffer = self.get_internal_buffer();
        let coder_bytes_avail = ibuffer.num_pop_bytes_avail();
        if coder_bytes_avail != 0 {
            let push_count = ibuffer.pop_data(output_buffer.split_at_mut(*output_offset).1);
            *output_offset += push_count;
            if ibuffer.num_pop_bytes_avail() != 0 {
                return BrotliResult::NeedsMoreOutput;
            }
        }
        return BrotliResult::ResultSuccess;
    }
    fn get_or_put_bit(&mut self,
                      bit: &mut bool,
                      prob_of_false: u8) {
        self.put_bit(*bit, prob_of_false)
    }
    fn get_or_put_nibble<U:CDFUpdater>(&mut self,
                                       nibble: &mut u8,
                                       prob: &CDF16<U>) {
        self.put_nibble(*nibble, prob);
    }
    fn close(&mut self) -> BrotliResult {
        self.flush();
        BrotliResult::ResultSuccess
    }
}
