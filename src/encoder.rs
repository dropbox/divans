use core::default::Default;
use probability::{CDFUpdater, CDF16, Prob};


trait FixedSizeByteQueue : Copy + Sized + Default {
      fn num_push_bytes_avail(&self) -> usize;
      fn num_pop_bytes_avail(&self) -> usize;
      fn push_data(&mut self, &[u8]) -> usize;
      fn pop_data(&mut self, &mut [u8]) -> usize;
}


trait Encoder<Queue: FixedSizeByteQueue> {
    // if it's a register, should have a get and a set and pass by value and clobber?
    fn get_internal_buffer(&mut self) -> &mut Queue;
    fn put_bit(&mut self,
               bit: bool,
               false_probability: &mut u8);
    fn put_nibble<U:CDFUpdater> (&mut self,
                                 nibble: u8,
                                 prob: &mut CDF16<U>) {
        let high_bit_prob = prob.cdf[7];
        let mut normalized_high_bit_prob = match prob.log_max() {
            None => ((high_bit_prob as i64) << 8) / prob.max(),
            Some(lmax) => ((high_bit_prob as i64)<< 8) >> lmax,
        } as u8;
        let high_bit = nibble & 8;
        self.put_bit(high_bit != 0, &mut normalized_high_bit_prob);
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
        let mut normalized_mid_prob = (((mid_prob as i64) << 8) / mid_max) as u8;
        self.put_bit(mid_bit != 0, &mut normalized_mid_prob);
        let bi_bit_probs = &tri_bit_probs[((nibble as usize & 4) as usize)..(((nibble as usize & 4) + 4) as usize)];
        let mut low_mid_bit_prob = (((bi_bit_probs[0] as u32 + bi_bit_probs[1] as u32) << 8)
            / (bi_bit_probs[0] as u32 + bi_bit_probs[1] as u32 + bi_bit_probs[2] as u32 + bi_bit_probs[3] as u32 + 1)) as u8;
        self.put_bit(((nibble as usize) & 4) != 0, &mut low_mid_bit_prob);
        let low_bit_prob = &bi_bit_probs[((nibble & 2) as usize )..(((nibble & 2) + 2) as usize)];
        let mut normalized_low_bit_prob = (((low_bit_prob[0] as u32) << 8) / (low_bit_prob[0] as u32 + low_bit_prob[1] as u32 + 1)) as u8;
        self.put_bit((nibble & 1) != 0, &mut normalized_low_bit_prob);
        prob.blend(nibble);
    }
    fn put_8bit(&mut self,
                bits: [bool;8], // should we make this a u8 and pull out the bits?
                true_probabilities: &mut [u8;8]) {
        for i in 0..true_probabilities.len() {
            self.put_bit(bits[i], &mut true_probabilities[i]);
        }
    }
    fn put_4nibble<U:CDFUpdater> (&mut self,
                                  nibbles: [u8;4],
                                  prob: &mut [CDF16<U>;4]){
        for i in 0..prob.len() {
            self.put_nibble(nibbles[i], &mut prob[i]);
        }
    }
    // output must have at least 64 bits of free space remaining for this function
    fn flush(&mut self);
}

trait Decoder<Queue:FixedSizeByteQueue> {
    // if it's a register, should have a get and a set and pass by value and clobber?
    fn get_internal_buffer(&mut self) -> &mut Queue;
    fn get_bit(&mut self, false_probability: &mut u8) -> bool;
    fn get_nibble<U:CDFUpdater> (&mut self, prob: &mut CDF16<U>) -> u8;
    fn get_8bit(&mut self, true_probabilities: &mut [u8;8]) -> [bool;8] {
        let mut ret = [false; 8];
        for i in 0..true_probabilities.len() {
            ret[i] = self.get_bit(&mut true_probabilities[i]);
        }
        ret
    }
    // input must have at least 64 bits inside unless we have reached the end
    fn get_4nibble<U:CDFUpdater> (&mut self, prob: &mut [CDF16<U>;4]) -> [u8;4] {
        let mut ret = [0u8; 4];
        for i in 0..prob.len() {
            ret[i] = self.get_nibble(&mut prob[i]);
        }
        ret
    }
}
