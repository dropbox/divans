use super::encoder::{
    EntropyEncoder,
    ByteQueue,
    RegisterQueue,
    EntropyDecoder,
};

#[derive(Default)]
struct DebugEncoder {
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
struct DebugDecoder {
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
}
