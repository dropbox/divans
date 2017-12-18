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
use core::default::Default;
use probability::{CDF16, ProbRange};
use interface::ArithmeticEncoderOrDecoder;
use super::BrotliResult;
pub trait ByteQueue {
      fn num_push_bytes_avail(&self) -> usize;
      fn num_pop_bytes_avail(&self) -> usize;
      fn push_data(&mut self, &[u8]) -> usize;
      fn pop_data(&mut self, &mut [u8]) -> usize;
}


pub type FixedRegister = u64;
pub struct RegisterQueue {
    data : FixedRegister,
    count : u8,
}
impl Default for RegisterQueue {
    fn default() -> Self {
        RegisterQueue{
            data:0,
            count:0,
        }
    }
}
impl ByteQueue for RegisterQueue {
    fn num_push_bytes_avail(&self) -> usize {
        core::mem::size_of::<FixedRegister>() - self.count as usize
    }
    fn num_pop_bytes_avail(&self) -> usize {
        self.count as usize
    }
    fn push_data(&mut self, data:&[u8]) -> usize {
        let byte_count_to_push = core::cmp::min(self.num_push_bytes_avail(),
                                                data.len());
        let offset = self.count << 3;
        self.count += byte_count_to_push as u8;
        let mut reg = self.data;
        for (i, data_iter) in data.split_at(byte_count_to_push).0.iter().enumerate() {
            reg |= FixedRegister::from(*data_iter) << (offset + ((i as u8) <<3));
        }
        self.data = reg;
        byte_count_to_push
    }
    fn pop_data(&mut self, data:&mut [u8]) -> usize {
        let byte_count_to_pop = core::cmp::min(self.num_pop_bytes_avail(),
                                               data.len());
        let mut local = [0u8;16];
        for i in 0..core::mem::size_of::<FixedRegister>() {
            local[i] = ((self.data >> (i << 3)) & 0xff) as u8;
        }
        data.split_at_mut(byte_count_to_pop).0.clone_from_slice(
            local.split_at(byte_count_to_pop).0);
        self.data >>= byte_count_to_pop << 3;
        self.count -= byte_count_to_pop as u8;
        byte_count_to_pop
    }
}


pub trait EntropyEncoder {
    type Queue:ByteQueue;
    // if it's a register, should have a get and a set and pass by value and clobber?
    fn get_internal_buffer(&mut self) -> &mut Self::Queue;
    fn put_bit(&mut self,
               bit: bool,
               prob_of_false: u8);
    fn put_nibble<C: CDF16>(&mut self,
                            nibble: u8,
                            prob: &C) -> ProbRange {
        let high_bit_prob = prob.cdf(7);
        let cdf_max = i32::from(prob.max());
        let normalized_high_bit_prob = match prob.log_max() {
            None => (i32::from(high_bit_prob) << 8) / cdf_max,
            Some(lmax) => (i32::from(high_bit_prob)<< 8) >> lmax,
        } as u8;
        let high_bit = nibble & 8;
        self.put_bit(high_bit != 0, normalized_high_bit_prob);
        let mid_max = if high_bit != 0 {cdf_max} else {i32::from(high_bit_prob)};
        let mid_min = if high_bit != 0 {i32::from(high_bit_prob)} else {0};
        let mid_prob = i32::from(prob.cdf((nibble & 8) + 3));
        let mid_bit = nibble & 4;
        let normalized_mid_prob = (((mid_prob -  mid_min) << 8) / (mid_max - mid_min as i32)) as u8;
        self.put_bit(mid_bit != 0, normalized_mid_prob);
        let lomid_min = if mid_bit != 0 {mid_prob} else {mid_min};
        let lomid_max = if mid_bit != 0 {mid_max} else {mid_prob};
        let lomid_prob = i32::from(prob.cdf((nibble & 12) + 1));
        let normalized_lomid_prob =(((lomid_prob -  lomid_min) << 8) / (lomid_max - lomid_min)) as u8;
        let lomid_bit = (nibble as usize) & 2;
        self.put_bit(lomid_bit != 0, normalized_lomid_prob);
        let lo_min = if lomid_bit != 0 {lomid_prob} else {lomid_min};
        let lo_max = if lomid_bit != 0 {lomid_max} else {lomid_prob};
        let lo_prob = i32::from(prob.cdf(nibble & 14));
        let normalized_lo_prob =(((lo_prob -  lo_min) << 8) / (lo_max - lo_min)) as u8;
        self.put_bit((nibble & 1) != 0, normalized_lo_prob);
        prob.sym_to_start_and_freq(nibble).range
    }
    fn put_8bit(&mut self,
                bits: [bool;8], // should we make this a u8 and pull out the bits?
                true_probabilities: [u8;8]) {
        for i in 0..true_probabilities.len() {
            self.put_bit(bits[i], true_probabilities[i]);
        }
    }
    // output must have at least 64 bits of free space remaining for this function
    fn flush(&mut self);
}

pub trait EntropyDecoder {
    type Queue:ByteQueue;
    // if it's a register, should have a get and a set and pass by value and clobber?
    fn get_internal_buffer(&mut self) -> &mut Self::Queue;
    fn get_bit(&mut self, prob_of_false: u8) -> bool;
    fn get_nibble<C: CDF16> (&mut self, prob: &C) -> (u8, ProbRange) {
        let high_bit_prob = prob.cdf(7);
        let cdf_max = i32::from(prob.max());
        let normalized_high_bit_prob = match prob.log_max() {
            None => (i32::from(high_bit_prob) << 8) / cdf_max,
            Some(lmax) => (i32::from(high_bit_prob) << 8) >> lmax,
        } as u8;
        let high_bit = (self.get_bit(normalized_high_bit_prob) as i32) << 3;
        let mut nibble = high_bit as u8;
        let mid_max = if high_bit != 0 {cdf_max} else {i32::from(high_bit_prob)};
        let mid_min = if high_bit != 0 {i32::from(high_bit_prob)} else {0};
        let mid_prob = i32::from(prob.cdf((nibble & 8) + 3));
        let normalized_mid_prob = (((mid_prob -  mid_min) << 8) / (mid_max - mid_min)) as u8;
        let mid_bit = (self.get_bit(normalized_mid_prob) as i32) << 2;
        nibble |= mid_bit as u8;
        let lomid_min = if mid_bit != 0 {mid_prob} else {mid_min};
        let lomid_max = if mid_bit != 0 {mid_max} else {mid_prob};
        let lomid_prob = i32::from(prob.cdf((nibble & 12) + 1));
        let normalized_lomid_prob =(((lomid_prob -  lomid_min) << 8) / (lomid_max - lomid_min)) as u8;
        let lomid_bit = (self.get_bit(normalized_lomid_prob) as i32) << 1;
        nibble |= lomid_bit as u8;
        let lo_min = if lomid_bit != 0 {lomid_prob} else {lomid_min};
        let lo_max = if lomid_bit != 0 {lomid_max} else {lomid_prob};
        let lo_prob = i32::from(prob.cdf(nibble & 14));
        let normalized_lo_prob = (((lo_prob -  lo_min) << 8) / (lo_max - lo_min)) as u8;
        nibble = nibble | self.get_bit(normalized_lo_prob) as u8;
        (nibble, prob.sym_to_start_and_freq(nibble).range)
        //println!("P({}) {} {} {} {} b {}", count, normalized_high_bit_prob, normalized_mid_prob, normalized_lomid_prob, lo_prob, nibble | self.get_bit(normalized_lo_prob) as u8);
    }
    fn get_8bit(&mut self, true_probabilities: [u8;8]) -> [bool;8] {
        let mut ret = [false; 8];
        for i in 0..true_probabilities.len() {
            ret[i] = self.get_bit(true_probabilities[i]);
        }
        ret
    }
    fn flush(&mut self) -> BrotliResult;
}


impl<Decoder:EntropyDecoder> ArithmeticEncoderOrDecoder for Decoder {
    #[inline(always)]
    fn drain_or_fill_internal_buffer(&mut self,
                                     input_buffer:&[u8],
                                     input_offset:&mut usize,
                                     _output_buffer:&mut [u8],
                                     _output_offset: &mut usize) -> BrotliResult {
        let ibuffer = self.get_internal_buffer();
        let coder_bytes_avail = ibuffer.num_push_bytes_avail();
        if coder_bytes_avail != 0 {
            let push_count = ibuffer.push_data(input_buffer.split_at(*input_offset).1);
            *input_offset += push_count;
            if ibuffer.num_push_bytes_avail() != 0 {
                return BrotliResult::NeedsMoreInput;
            }
        }
        BrotliResult::ResultSuccess
    }
    fn get_or_put_bit_without_billing(&mut self,
                                      bit: &mut bool,
                                      prob_of_false: u8) {
        *bit = self.get_bit(prob_of_false);
    }
    #[inline(always)]
    fn get_or_put_nibble_without_billing<C: CDF16>(&mut self,
                                                   nibble: &mut u8,
                                                   prob: &C) -> ProbRange {
        let (nib, ret) = self.get_nibble(prob);
        *nibble = nib;
        ret
    }
    fn close(&mut self) -> BrotliResult {
        self.flush()
    }
}

macro_rules! arithmetic_encoder_or_decoder_methods(
    () => {
            fn drain_or_fill_internal_buffer(&mut self,
                                             _input_buffer:&[u8],
                                             _input_offset:&mut usize,
                                             output_buffer:&mut [u8],
                                             output_offset: &mut usize) -> BrotliResult {
                let ibuffer = self.get_internal_buffer();
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
            fn get_or_put_bit_without_billing(&mut self,
                                              bit: &mut bool,
                                              prob_of_false: u8) {
                self.put_bit(*bit, prob_of_false)
            }
            fn get_or_put_nibble_without_billing<C: CDF16>(&mut self,
                                                           nibble: &mut u8,
                                                           prob: &C) -> ::probability::ProbRange {
                self.put_nibble(*nibble, prob)
            }
            fn close(&mut self) -> BrotliResult {
                self.flush();
                BrotliResult::ResultSuccess
            }
        }
);

mod test {
    use super::ByteQueue;
    use super::{EntropyEncoder, EntropyDecoder};
    use super::super::BrotliResult;
    #[allow(unused_imports)]
    use probability::{CDF16, FrequentistCDF16, BlendCDF16, Speed};
    #[allow(unused)]
    struct MockByteQueue{}
    impl ByteQueue for MockByteQueue {
        fn num_push_bytes_avail(&self) -> usize {0}
        fn num_pop_bytes_avail(&self) -> usize {0}
        fn push_data(&mut self, _b:&[u8]) -> usize {0}
        fn pop_data(&mut self, _b:&mut [u8]) -> usize {0}
    }
    #[allow(unused)]
    struct MockBitCoder {
        calls_to_put_bit: [[(bool, u8);4]; 16],
        num_calls: usize,
        queue: MockByteQueue,
    }
    impl EntropyEncoder for MockBitCoder {
        type Queue = MockByteQueue;
        fn get_internal_buffer(&mut self) -> &mut MockByteQueue {
            &mut self.queue
        }
        fn put_bit(&mut self, bit: bool, prob_of_false: u8) {
            self.calls_to_put_bit[self.num_calls >> 2][self.num_calls&3] = (bit, prob_of_false);
            self.num_calls += 1;
        }
        fn flush(&mut self){}
    }
    impl EntropyDecoder for MockBitCoder {
        type Queue = MockByteQueue;
        fn get_internal_buffer(&mut self) -> &mut MockByteQueue {
            &mut self.queue
        }
        fn get_bit(&mut self, prob_of_false: u8) -> bool {
            let bit = self.calls_to_put_bit[self.num_calls >> 2][self.num_calls&3].0;
            self.calls_to_put_bit[self.num_calls >> 2][self.num_calls&3] = (bit, prob_of_false);
            self.num_calls += 1;
            bit
        }
        fn flush(&mut self) -> BrotliResult {
            BrotliResult::ResultSuccess
        }
    }
    #[allow(unused)]
    fn test_get_prob<C: CDF16>(cdf: &C,
                               prob_start:u8,
                               prob_mid:u8,
                               prob_end:u8) -> u8 {
        let hi = if prob_end == 16 {
            i64::from(cdf.max())
        } else {
            i64::from(cdf.cdf(prob_end as u8 - 1))
        };
        let lo = if prob_start == 0 {
            0
        } else {
            i64::from(cdf.cdf(prob_start as u8 - 1))
        };
        let mid = i64::from(cdf.cdf(prob_mid as u8 - 1));
        //println!("Test get prob MID : {:} [{:}] HIGH {:} LO: {:} NORM {:}",
        //          mid,    ((prob_start as usize + prob_end as usize - 1)>>1),
        //          hi, lo,         (((mid - lo) << 8) / (cdf.max() - lo)) as u8);

        (((mid - lo) << 8) / (hi - lo)) as u8
    }
    #[allow(unused)]
    fn validate_call_to_put<C: CDF16>(calls: [(bool, u8);4],
                                      cdf: &C,
                                      sym: u8) {
        for i in 0..4 {
            if calls[i].0 {
                assert_eq!(sym & (1 << (3 - i)), (1 << (3 - i)));
            } else {
                assert_eq!(sym & (1 << (3 - i)), 0);
            }
        }
        let hi_prob = test_get_prob(cdf, 0, 8, 16);
        let himed_prob = test_get_prob(cdf, sym & 8, (sym & 8) + 4,(sym&8) + 8);
        let lomed_prob = test_get_prob(cdf, sym & 12,(sym & 12) + 2, (sym & 12) + 4);
        let lo_prob = test_get_prob(cdf, sym & 14,(sym & 14) + 1, (sym&14) + 2);
        assert_eq!(calls[0].1, hi_prob);
        assert_eq!(calls[1].1, himed_prob); //FIXME: make these compares operate
        assert_eq!(calls[2].1, lomed_prob);
        assert_eq!(calls[3].1, lo_prob);
    }
    #[cfg(test)]
    #[test]
    fn test_put_nibble() {
        let mut cdf = FrequentistCDF16::default();
        let mut bcdf = BlendCDF16::default();
        for i in 0..16 {
            for j in 0..i {
                cdf.blend(j as u8, Speed::MED);
                bcdf.blend(j as u8, Speed::MED);
            }
        }
        println!("{:?}", cdf.float_array());
        let mut mock_encoder = MockBitCoder{
            calls_to_put_bit: [[(false,0);4];16],
            num_calls:0,
            queue:MockByteQueue{},
        };
        for i in 0..16 {
            mock_encoder.put_nibble(i as u8, &cdf);
        }
        for i in 0..16 {
            println!("Validating {:}", i);
            validate_call_to_put(mock_encoder.calls_to_put_bit[i],
                                 &cdf,
                                 i as u8);
        }
        mock_encoder = MockBitCoder{
            calls_to_put_bit: [[(false,0);4];16],
            num_calls:0,
            queue:MockByteQueue{},
        };
        for i in 0..16 {
            mock_encoder.put_nibble(i as u8, &bcdf);
        }
        for i in 0..16 {
            validate_call_to_put(mock_encoder.calls_to_put_bit[i],
                                 &bcdf,
                                 i as u8);
        }
    }
    #[cfg(test)]
    #[test]
    fn test_get_nibble() {
        let mut cdf = FrequentistCDF16::default();
        let mut bcdf = BlendCDF16::default();
        for i in 0..16 {
            for j in 0..i {
                cdf.blend(j as u8, Speed::MED);
                bcdf.blend(j as u8, Speed::MED);
            }
        }
        println!("{:?}", cdf.float_array());
        let mut mock_decoder = MockBitCoder{
            calls_to_put_bit: [[(false,0);4];16],
            num_calls:0,
            queue:MockByteQueue{},
        };
        for i in 0..16 {
            mock_decoder.calls_to_put_bit[i][0].0 = (i & 8) != 0;
            mock_decoder.calls_to_put_bit[i][1].0 = (i & 4) != 0;
            mock_decoder.calls_to_put_bit[i][2].0 = (i & 2) != 0;
            mock_decoder.calls_to_put_bit[i][3].0 = (i & 1) != 0;
        }
        for i in 0..16 {
            let nib = mock_decoder.get_nibble(&cdf);
            assert_eq!(nib.0, i as u8);
        }
        for i in 0..16 {
            println!("Validating {:}", i);
            validate_call_to_put(mock_decoder.calls_to_put_bit[i],
                                 &cdf,
                                 i as u8);
        }
        mock_decoder = MockBitCoder{
            calls_to_put_bit: [[(false,0);4];16],
            num_calls:0,
            queue:MockByteQueue{},
        };
        for i in 0..16 {
            mock_decoder.calls_to_put_bit[i][0].0 = (i & 8) != 0;
            mock_decoder.calls_to_put_bit[i][1].0 = (i & 4) != 0;
            mock_decoder.calls_to_put_bit[i][2].0 = (i & 2) != 0;
            mock_decoder.calls_to_put_bit[i][3].0 = (i & 1) != 0;
        }
        for i in 0..16 {
            let nib = mock_decoder.get_nibble(&bcdf).0;
            assert_eq!(nib, i as u8)
        }
        for i in 0..16 {
            validate_call_to_put(mock_decoder.calls_to_put_bit[i],
                                 &bcdf,
                                 i as u8);
        }
    }
}
