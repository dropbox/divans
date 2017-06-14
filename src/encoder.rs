use super::probability::{CDFUpdater, CDF16};
use super::alloc::{SliceWrapperMut, SliceWrapper};
mod OptionA {
    use probability::{CDFUpdater, CDF16};
    trait Encoder {
        fn put_bit(&mut self,
                   bit: bool,
                   true_probability: u8,
                   offset: &mut usize,
                   output: &mut[u8]) -> Result<(),()>;
        fn put_nibble<U:CDFUpdater> (&mut self,
                                   nibble: u8,
                                   prob: CDF16<U>,
                                   offset: &mut usize,
                                   output: &mut[u8]) -> Result<(),()>;
        // no further calls to put after flush. Err return value indicates
        // need to call flush again until Ok
        fn flush(offset: &mut usize,
                 output: &mut[u8]) -> Result<(), ()>;
    }

    trait Decoder {
        fn get_bit(&mut self,
                   true_probability: u8,
                   offset: &mut usize,
                   input: &[u8]) -> Result<bool, ()>;
        fn get_nibble<U:CDFUpdater> (&mut self,
                                   prob: CDF16<U>,
                                   offset: &mut usize,
                                   input: &[u8],
                                   is_eof: bool) -> Result<u8, ()>;
       
    }
}
mod OptionB {
    use probability::{CDFUpdater, CDF16};

    trait Encoder {
        // output must have at least 16 bits of free space remaining for this function
        fn put_bit(&mut self,
                   bit: bool,
                   true_probability: u8,
                   offset: &mut usize,
                   output: &mut[u8]);
        // output must have at least 64 bits of free space remaining for this function
        fn put_nibble<U:CDFUpdater> (&mut self,
                                   nibble: u8,
                                   prob: CDF16<U>,
                                   offset: &mut usize,
                                   output: &mut[u8]);
        // output must have at least 64 bits of free space remaining for this function
        fn flush(offset: &mut usize, output: &mut[u8]);
    }

    trait Decoder {
        // input must have at least 64 bits inside unless we have reached the end
        fn get_bit(&mut self,
                   true_probability: u8,
                   offset: &mut usize,
                   input: &[u8]) -> bool;
        // input must have at least 64 bits inside unless we have reached the end
        fn get_nibble<U:CDFUpdater> (&mut self,
                                   prob: CDF16<U>,
                                   offset: &mut usize,
                                   input: &[u8]) -> u8;
    }
}
mod OptionC {
    use probability::{CDFUpdater, CDF16};

    trait Encoder {
        fn need_buffer_space(&mut self) -> Option<usize>;
        fn prepare_buffer(&mut self,
                          offset: &mut usize,
                          output: &mut [u8]) -> Result<(),()>;
        fn put_bit(&mut self,
                   bit: bool,
                   true_probability: u8);
        // output must have at least 64 bits of free space remaining for this function
        fn put_nibble<U:CDFUpdater> (&mut self,
                                   nibble: u8,
                                   prob: CDF16<U>);
        // output must have at least 64 bits of free space remaining for this function
        fn flush(&mut self, offset: &mut usize, output: &mut[u8]);
    }

    trait Decoder {
        // check this function before every call to get_nibble (or 4 calls to get_bit)
        fn need_buffer_space(&mut self) -> Option<usize>;
        fn prepare_buffer(&mut self,
                          offset: &mut usize,
                          input: &mut [u8]);
        // input must have at least 64 bits inside unless we have reached the end
        fn get_bit(&mut self, true_probability: u8) -> bool;
        // input must have at least 64 bits inside unless we have reached the end
        fn get_nibble<U:CDFUpdater> (&mut self, prob: CDF16<U>) -> u8;
    }
}


trait FixedSizeByteQueue {
      fn num_push_bytes_avail(&self) -> usize;
      fn num_pop_bytes_avail(&self) -> usize;
      fn push_data(&mut self, &[u8]) -> usize;
      fn pop_data(&mut self, &mut [u8]) -> usize;
}

mod OptionD {
    use probability::{CDFUpdater, CDF16};
    use super::FixedSizeByteQueue;
    use alloc::{SliceWrapper,SliceWrapperMut};
    //use super::{CDFUpdater, CDF16, SliceWrapperMut, SliceWrapper};

    trait Encoder<Queue: FixedSizeByteQueue> {
        fn get_buffer(&mut self) -> &mut Queue;
        fn put_bit(&mut self,
                   bit: bool,
                   true_probability: &mut u8);
        // output must have at least 64 bits of free space remaining for this function
        fn put_nibble<U:CDFUpdater> (&mut self,
                                   nibble: u8,
                                   prob: &mut CDF16<U>);
           
        fn put_nibbles<T:SliceWrapper<u8>,
                       U:CDFUpdater,
                       V:SliceWrapperMut<CDF16<U>>> (&mut self,
                                                     nibbles: T,
                                                     probs: V);
        // output must have at least 8 * bits.len() bits of free space remaining for this function
        fn put_bits<T:SliceWrapper<bool>,
                    U:SliceWrapperMut<u8>> (&mut self,
                                            bits: T,
                                            true_probs: U);
        // output must have at least 64 bits of free space remaining for this function
        fn flush(&mut self);
    }

    trait Decoder<Queue:FixedSizeByteQueue> {
        fn get_buffer(&mut self) -> &mut Queue;
        // input must have at least 64 bits inside unless we have reached the end
        fn get_bit(&mut self, true_probability: &mut u8) -> bool;
        // input must have at least 64 bits inside unless we have reached the end
        fn get_nibble<U:CDFUpdater> (&mut self, prob: &mut CDF16<U>) -> u8;
        fn get_nibbles<T:SliceWrapperMut<u8>,
                       U:CDFUpdater,
                       V:SliceWrapperMut<CDF16<U>>> (&mut self,
                                                     out_nibbles:T,
                                                     probs: V);
        // output must have at least 8 * bits.len() bits of free space remaining for this function
        fn get_bits<T:SliceWrapperMut<bool>,
                    U:SliceWrapperMut<u8>> (&mut self,
                                            out_bits: T,
                                            true_probs: U);
    }
}

mod OptionE {
    use probability::{CDFUpdater, CDF16};
    use super::FixedSizeByteQueue;
    use alloc::{SliceWrapper,SliceWrapperMut};

   // use super::{CDFUpdater, CDF16, SliceWrapperMut, SliceWrapper};

    trait Encoder<Queue: FixedSizeByteQueue> {
        // if it's a register, should have a get and a set and pass by value and clobber?
        fn get_internal_buffer(&mut self) -> &mut Queue;
        fn put_bit(&mut self,
                   bit: bool,
                   true_probability: u8);
        // output must have at least 64 bits of free space remaining for this function
        fn put_nibble<U:CDFUpdater> (&mut self,
                                   nibble: u8,
                                   prob: CDF16<U>);
        fn put_8bit(&mut self,
                   bits: [bool;8], // should we make this a u8 and pull out the bits?
                   true_probabilities: &mut [u8;8]);
        // output must have at least 64 bits of free space remaining for this function
        fn put_4nibble<U:CDFUpdater> (&mut self,
                                      nibbles: [u8;4],
                                      prob: &mut [CDF16<U>;4]);
        // output must have at least 64 bits of free space remaining for this function
        fn flush(&mut self);
    }

    trait Decoder<Queue:FixedSizeByteQueue> {
        // if it's a register, should have a get and a set and pass by value and clobber?
        fn get_buffer(&mut self) -> &mut Queue;
        // input must have at least 64 bits inside unless we have reached the end
        fn get_bit(&mut self, true_probability: &mut u8) -> bool;
        // input must have at least 64 bits inside unless we have reached the end
        fn get_nibble<U:CDFUpdater> (&mut self, prob: &mut CDF16<U>) -> u8;
        // input must have at least 64 bits inside unless we have reached the end
        fn get_8bit(&mut self, true_probability: &mut [u8;8]) -> [bool;8];
        // input must have at least 64 bits inside unless we have reached the end
        fn get_4nibble<U:CDFUpdater> (&mut self, prob: &mut [CDF16<U>;4]) -> [u8;4];
    }
}
