#[allow(unused)]
use core::default::Default;
use probability::{CDFUpdater, CDF16, Prob};


trait ByteQueue {
      fn num_push_bytes_avail(&self) -> usize;
      fn num_pop_bytes_avail(&self) -> usize;
      fn push_data(&mut self, &[u8]) -> usize;
      fn pop_data(&mut self, &mut [u8]) -> usize;
}


trait Encoder<Queue: ByteQueue> {
    // if it's a register, should have a get and a set and pass by value and clobber?
    fn get_internal_buffer(&mut self) -> &mut Queue;
    fn put_bit(&mut self,
               bit: bool,
               prob_of_false: u8);
    fn put_nibble<U:CDFUpdater> (&mut self,
                                 nibble: u8,
                                 prob: &CDF16<U>) {
        let high_bit_prob = prob.cdf[7];
        let normalized_high_bit_prob = match prob.log_max() {
            None => ((high_bit_prob as i64) << 8) / prob.max(),
            Some(lmax) => ((high_bit_prob as i64)<< 8) >> lmax,
        } as u8;
        let high_bit = nibble & 8;
        let high_bit_mask = if high_bit == 0 { 0u8 } else { 255u8 };
        self.put_bit(high_bit != 0, high_bit_mask ^ normalized_high_bit_prob);
        let mid_max = if high_bit != 0 {prob.max()} else {high_bit_prob as i64};
        let mid_min = if high_bit != 0 {high_bit_prob} else {0};
        let tri_bit_probs : [Prob; 8] = [
            prob.cdf[(nibble & 8) as usize] - mid_min,
            prob.cdf[(nibble & 8) as usize + 1] - prob.cdf[(nibble as usize & 8) as usize],
            prob.cdf[(nibble & 8) as usize + 2] - prob.cdf[(nibble as usize & 8) + 1],
            prob.cdf[(nibble & 8) as usize + 3] - prob.cdf[(nibble as usize & 8) + 2],
            prob.cdf[(nibble & 8) as usize + 4] - prob.cdf[(nibble as usize & 8) + 3],
            prob.cdf[(nibble & 8) as usize + 5] - prob.cdf[(nibble as usize & 8) + 4],
            prob.cdf[(nibble & 8) as usize + 6] - prob.cdf[(nibble as usize & 8) + 5],
            (mid_max - prob.cdf[(nibble&8) as usize + 6] as i64) as Prob];
        let mid_prob = prob.cdf[(nibble & 8) as usize + 3];
        let mid_bit = nibble & 4;
        let normalized_mid_prob = (((mid_prob as i64) << 8) / mid_max) as u8;
        self.put_bit(mid_bit != 0, normalized_mid_prob);
        let bi_bit_probs = &tri_bit_probs[((nibble as usize & 4) as usize)..(((nibble as usize & 4) + 4) as usize)];
        let low_mid_bit_prob = (((bi_bit_probs[0] as u32 + bi_bit_probs[1] as u32) << 8)
            / (bi_bit_probs[0] as u32 + bi_bit_probs[1] as u32 + bi_bit_probs[2] as u32 + bi_bit_probs[3] as u32 + 1)) as u8;
        self.put_bit(((nibble as usize) & 2) != 0, low_mid_bit_prob);
        let low_bit_prob = &bi_bit_probs[((nibble & 2) as usize )..(((nibble & 2) + 2) as usize)];
        let normalized_low_bit_prob = (((low_bit_prob[0] as u32) << 8) / (low_bit_prob[0] as u32 + low_bit_prob[1] as u32 + 1)) as u8;
        self.put_bit((nibble & 1) != 0, normalized_low_bit_prob);
    }
    fn put_8bit(&mut self,
                bits: [bool;8], // should we make this a u8 and pull out the bits?
                true_probabilities: [u8;8]) {
        for i in 0..true_probabilities.len() {
            self.put_bit(bits[i], true_probabilities[i]);
        }
    }
    fn put_4nibble<U:CDFUpdater> (&mut self,
                                  nibbles: [u8;4],
                                  prob: &[CDF16<U>;4]){
        for i in 0..prob.len() {
            self.put_nibble(nibbles[i], &prob[i]);
        }
    }
    // output must have at least 64 bits of free space remaining for this function
    fn flush(&mut self);
}

trait Decoder<Queue:ByteQueue> {
    // if it's a register, should have a get and a set and pass by value and clobber?
    fn get_internal_buffer(&mut self) -> &mut Queue;
    fn get_bit(&mut self, prob_of_false: u8) -> bool;
    fn get_nibble<U:CDFUpdater> (&mut self, prob: &CDF16<U>) -> u8;
    fn get_8bit(&mut self, true_probabilities: [u8;8]) -> [bool;8] {
        let mut ret = [false; 8];
        for i in 0..true_probabilities.len() {
            ret[i] = self.get_bit(true_probabilities[i]);
        }
        ret
    }
    // input must have at least 64 bits inside unless we have reached the end
    fn get_4nibble<U:CDFUpdater> (&mut self, prob: &[CDF16<U>;4]) -> [u8;4] {
        let mut ret = [0u8; 4];
        for i in 0..prob.len() {
            ret[i] = self.get_nibble(&prob[i]);
        }
        ret
    }
}



mod test {
    use super::ByteQueue;
    use super::Encoder;
    #[allow(unused_imports)]
    use probability::{CDF16, FrequentistCDFUpdater, BlendCDFUpdater, CDFUpdater};
    #[allow(unused)]
    struct MockByteQueue{}
    impl ByteQueue for MockByteQueue {
        fn num_push_bytes_avail(&self) -> usize {0}
        fn num_pop_bytes_avail(&self) -> usize {0}
        fn push_data(&mut self, _b:&[u8]) -> usize {0}
        fn pop_data(&mut self, _b:&mut [u8]) -> usize {0}
    }
    #[allow(unused)]
    struct MockBitEncoder {
        calls_to_put_bit: [[(bool, u8);4]; 16],
        num_calls: usize,
        queue: MockByteQueue,
    }
    impl Encoder<MockByteQueue> for MockBitEncoder {
        fn get_internal_buffer(&mut self) -> &mut MockByteQueue {
            &mut self.queue
        }
        fn put_bit(&mut self, bit: bool, prob_of_false: u8) {
            self.calls_to_put_bit[self.num_calls >> 2][self.num_calls&3] = (bit, prob_of_false);
            self.num_calls += 1;
        }
        fn flush(&mut self){}
    }
    #[allow(unused)]
    fn test_get_prob<T:CDFUpdater>(cdf: &CDF16<T>,
                                   prob_start:u8,
                                   prob_end:u8) -> u8 {
        let hi;
        if prob_end == 16 {
            hi = cdf.max() as i64;
        } else {
            hi = cdf.cdf[prob_end as usize - 1] as i64;
        }
        let lo;
        if prob_start == 0 {
            lo = 0;
        } else {
            lo = cdf.cdf[prob_start as usize - 1] as i64;
        }
        match cdf.log_max() {
            None => (((hi - lo) << 8) / cdf.max()) as u8,
            Some(lmax) => (((hi - lo) << 8) >> lmax) as u8 ,
        }
        
    }
    #[allow(unused)]
    fn validate_call_to_put<T:CDFUpdater>(calls: [(bool, u8);4],
                                          cdf: &CDF16<T>,
                                          sym: u8) {
        for i in 0..4 {
            if calls[i].0 {
                assert_eq!(sym & (1 << (3 - i)), (1 << (3 - i)));
            } else {
                assert_eq!(sym & (1 << (3 - i)), 0);
            }
        }
        let hi_prob = test_get_prob(cdf, sym & 8,(sym & 8) + 8);
        let himed_prob = test_get_prob(cdf, sym & 12,(sym & 12) + 4);
        let lomed_prob = test_get_prob(cdf, sym & 14,(sym & 14) + 2);
        let lo_prob = test_get_prob(cdf, sym & 15,(sym & 15) + 1);
        assert_eq!(calls[0].1, hi_prob);
        //assert_eq!(calls[1].1, himed_prob); //FIXME: make these compares operate
        //assert_eq!(calls[2].1, lomed_prob);
        //assert_eq!(calls[3].1, lo_prob);
    }
    #[cfg(test)]
    #[test]
    fn test_put_nibble() {
        let mut cdf = CDF16::<FrequentistCDFUpdater>::default();
        let mut bcdf = CDF16::<BlendCDFUpdater>::default();
        for i in 0..16 {
            for j in 0..i {
                cdf.blend(j as u8);
                bcdf.blend(j as u8);
            }
        }
        println!("{:?}", cdf.float_array());
        let mut mock_encoder = MockBitEncoder{
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
        mock_encoder = MockBitEncoder{
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
}
