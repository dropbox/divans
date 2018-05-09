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
use super::arithmetic_coder::{
    EntropyEncoder,
    ByteQueue,
    RegisterQueue,
    EntropyDecoder,
};
use probability::CDF16;
use super::interface::ArithmeticEncoderOrDecoder;
use super::DivansResult;
#[derive(Default)]
pub struct DebugEncoder {
    buffer: RegisterQueue,
}


impl EntropyEncoder for DebugEncoder {
    type Queue = RegisterQueue;
    fn get_internal_buffer_mut(&mut self) -> &mut RegisterQueue {
        &mut self.buffer
    }
    fn get_internal_buffer(&self) -> &RegisterQueue {
        &self.buffer
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
    fn get_internal_buffer_mut(&mut self) -> &mut RegisterQueue {
        &mut self.buffer
    }
    fn get_internal_buffer(&self) -> &RegisterQueue {
        &self.buffer
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
    fn flush(&mut self) -> DivansResult {
        DivansResult::Success
    }
}

impl DebugEncoder {
    fn mov_internal(&mut self) -> Self {
        core::mem::replace(self, DebugEncoder::default())
    }
}
impl ArithmeticEncoderOrDecoder for DebugEncoder {
    arithmetic_encoder_or_decoder_methods!();
}
