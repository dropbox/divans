use super::probability::{CDFUpdater, CDF16};

mod OptionA {
    trait Encoder {
        fn put_bit(&mut self,
                   bit: bool,
                   true_probability: u8,
                   offset: &mut usize,
                   output: &mut[u8]) -> Result<(),()>;
        fn put_nibble<CDFUpdater> (&mut self,
                                   nibble: u8,
                                   prob: CDF16<CDFUpdater>,
                                   offset: &mut usize,
                                   output: &mut[u8]) -> Result<(),()>;
        // no further calls to put after flush. Err return value indicates
        // need to call flush again until Ok
        fn flush(offset: &mut usize,
                 output &mut[u8]) -> Result<(), ()>;
    }

    trait Decoder {
        fn get_bit(&mut self,
                   true_probability: u8,
                   offset: &mut usize,
                   input: &[u8]) -> Result<bool, ()>;
        fn get_nibble<CDFUpdater> (&mut self,
                                   prob: CDF16<CDFUpdater>,
                                   offset: &mut usize,
                                   input: &[u8],
                                   is_eof: bool) -> Result<u8, ()>;
       
    }
}
mod OptionB {
    trait Encoder {
        // output must have at least 16 bits of free space remaining for this function
        fn put_bit(&mut self,
                   bit: bool,
                   true_probability: u8,
                   &mut offset: usize,
                   output: &mut[u8]);
        // output must have at least 64 bits of free space remaining for this function
        fn put_nibble<CDFUpdater> (&mut self,
                                   nibble: u8,
                                   prob: CDF16<CDFUpdater>,
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
        fn get_nibble<CDFUpdater> (&mut self,
                                   prob: CDF16<CDFUpdater>,
                                   offset: &mut usize,
                                   input: &[u8]) -> u8;
    }
}
mod OptionC {
    trait Encoder {
        fn need_buffer_space(&mut self) -> Option<usize>;
        fn prepare_buffer(&mut self,
                          offset: &mut usize,
                          output: &mut [u8]) -> Result<(),()>;
        fn put_bit(&mut self,
                   bit: bool,
                   true_probability: u8);
        // output must have at least 64 bits of free space remaining for this function
        fn put_nibble<CDFUpdater> (&mut self,
                                   nibble: u8,
                                   prob: CDF16<CDFUpdater>);
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
        fn get_nibble<CDFUpdater> (&mut self, prob: CDF16<CDFUpdater>) -> u8;
    }
}
