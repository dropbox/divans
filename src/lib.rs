#![no_std]
#[cfg(test)]
#[macro_use]
extern crate std;

#[cfg(not(test))]
#[cfg(feature="billing")]
#[macro_use]
extern crate std;
extern crate alloc_no_stdlib as alloc;
extern crate brotli_decompressor;

mod interface;
mod probability;
#[macro_use]
mod priors;
#[macro_use]
mod encoder;
mod debug_encoder;
mod cmd_to_raw;
mod codec;
mod cmd_to_divans;
mod divans_to_raw;
mod billing;
pub use codec::COMMAND_LINE_ENFORCE_LEGACY_ENCODING; 
mod ans;
pub use brotli_decompressor::{BrotliResult};
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use interface::{BlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator};
pub use cmd_to_raw::DivansRecodeState;
pub use codec::CMD_BUFFER_SIZE;
pub use divans_to_raw::DecoderSpecialization;
pub use cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec};

const HEADER_LENGTH: usize = 16;
const MAGIC_NUMBER:[u8;4] = [0xff, 0xe5,0x8c, 0x9f];

macro_rules! DefaultArithmeticEncoder(
    () => { ans::EntropyEncoderANS }
);
macro_rules! DefaultArithmeticDecoder(
    () => { ans::EntropyDecoderANS }
);

#[cfg(feature="blend")]
pub type DefaultCDF16 = probability::BlendCDF16;
#[cfg(not(feature="blend"))]
pub type DefaultCDF16 = probability::FrequentistCDF16;

pub use probability::CDF2;


#[cfg(not(feature="billing"))]
macro_rules! SelectedArithmeticEncoder(
    ($type: ident) => { DefaultArithmeticEncoder!()<$type> }
);

#[cfg(not(feature="billing"))]
macro_rules! NewSelectedArithmeticEncoder(
    ($type: ident, $val: ident) => { DefaultArithmeticEncoder!()::<$val>new($val)  }
);

#[cfg(not(feature="billing"))]
macro_rules! SelectedArithmeticDecoder(
    ($val: expr) => { DefaultArithmeticDecoder!($val) }
);

#[cfg(not(feature="billing"))]
macro_rules! NewSelectedArithmeticDecoder(
    ($type: ident, $val: ident) => { DefaultArithmeticDecoder!()::<$val>new($val)  }
);


#[cfg(feature="billing")]
macro_rules! SelectedArithmeticEncoder(
    ($val: ident) => { billing::BillingArithmeticCoder<DefaultArithmeticEncoder!($val)> }
);
#[cfg(feature="billing")]
macro_rules! NewSelectedArithmeticEncoder(
    ($type: ident, $val: ident) => { billing::BillingArithmeticCoder<DefaultArithmeticEncoder!()<$val>>::new($val)  }
);


#[cfg(feature="billing")]
macro_rules! SelectedArithmeticDecoder(
    ($val: ident) => { billing::BillingArithmeticCoder<DefaultArithmeticDecoder!($val)> }
);
#[cfg(feature="billing")]
macro_rules! NewSelectedArithmeticDecoder(
    ($type: ident, $val: ident) => { billing::BillingArithmeticCoder<DefaultArithmeticDecoder!()<$val>>::new($val)  }
);



pub struct DivansCompressor<AllocU8:Allocator<u8>,
                            AllocCDF2:Allocator<probability::CDF2>,
                            AllocCDF16:Allocator<DefaultCDF16>> {
    codec: DivansCodec<SelectedArithmeticEncoder!(AllocU8), EncoderSpecialization, DefaultCDF16, AllocU8, AllocCDF2, AllocCDF16>,
    header_progress: usize,
    window_size: u8,
}
impl<AllocU8:Allocator<u8>, AllocCDF2:Allocator<probability::CDF2>, AllocCDF16:Allocator<DefaultCDF16>> DivansCompressor<AllocU8,
                                                                                                                         AllocCDF2,
                                                                                                                         AllocCDF16> {
    pub fn new(mut m8: AllocU8, mcdf2:AllocCDF2, mcdf16:AllocCDF16,mut window_size: usize) -> Self {
        if window_size < 10 {
            window_size = 10;
        }
        if window_size > 24 {
            window_size = 24;
        }
        let enc = NewSelectedArithmeticEncoder(AllocU8, &mut m8);
        DivansCompressor::<AllocU8, AllocCDF2, AllocCDF16> {
            codec:DivansCodec::<SelectedArithmeticEncoder!(AllocU8), EncoderSpecialization, DefaultCDF16, AllocU8, AllocCDF2, AllocCDF16>::new(
                m8,
                mcdf2,
                mcdf16,
                enc,
                EncoderSpecialization::new(),
                window_size,
            ),
            header_progress: 0,
            window_size: window_size as u8,
        }
    }
}

fn make_header(window_size: u8) -> [u8; HEADER_LENGTH] {
    let mut retval = [0u8; HEADER_LENGTH];
    retval[0..MAGIC_NUMBER.len()].clone_from_slice(&MAGIC_NUMBER[..]);
    retval[5] = window_size;
    retval
}

impl<AllocU8:Allocator<u8>, AllocCDF2:Allocator<probability::CDF2>, AllocCDF16:Allocator<DefaultCDF16>> DivansCompressor<AllocU8, AllocCDF2,
                                                                                                                         AllocCDF16> {
    fn write_header(&mut self, output: &mut[u8],
                    output_offset:&mut usize) -> BrotliResult {
        let bytes_avail = output.len() - *output_offset;
        if bytes_avail + self.header_progress < HEADER_LENGTH {
            output.split_at_mut(*output_offset).1.clone_from_slice(
                &make_header(self.window_size)[self.header_progress..
                                              (self.header_progress + bytes_avail)]);
            *output_offset += bytes_avail;
            return BrotliResult::NeedsMoreOutput;
        }
        output[*output_offset..(*output_offset + HEADER_LENGTH - self.header_progress)].clone_from_slice(
                &make_header(self.window_size)[self.header_progress..]);
        *output_offset += HEADER_LENGTH - self.header_progress;
        self.header_progress = HEADER_LENGTH;
        BrotliResult::ResultSuccess
    }
}

impl<AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> Compressor for DivansCompressor<AllocU8, AllocCDF2, AllocCDF16>   {
    fn encode<SliceType:SliceWrapper<u8>+Default>(&mut self,
                                          input:&[Command<SliceType>],
                                          input_offset : &mut usize,
                                          output :&mut[u8],
                                          output_offset: &mut usize) -> BrotliResult{
        if self.header_progress != HEADER_LENGTH {
            match self. write_header(output, output_offset) {
                BrotliResult::ResultSuccess => {},
                res => return res,
            }
        }
        let mut unused: usize = 0;
        self.codec.encode_or_decode(&[],
                                    &mut unused,
                                    output,
                                    output_offset,
                                    input,
                                    input_offset)
    }
    fn flush(&mut self,
             output: &mut [u8],
             output_offset: &mut usize) -> BrotliResult {
        if self.header_progress != HEADER_LENGTH {
            match self.write_header(output, output_offset) {
                BrotliResult::ResultSuccess => {},
                res => return res,
            }
        }
        self.codec.flush(output, output_offset)
    }
}


pub struct HeaderParser<AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<probability::CDF2>,
                        AllocCDF16:Allocator<DefaultCDF16>> {
    header:[u8;HEADER_LENGTH],
    read_offset: usize,
    m8: Option<AllocU8>,
    mcdf2: Option<AllocCDF2>,
    mcdf16: Option<AllocCDF16>,
}
impl<AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>>HeaderParser<AllocU8, AllocCDF2, AllocCDF16> {
    pub fn parse_header(&mut self)->Result<usize, BrotliResult>{
        if self.header[0] != MAGIC_NUMBER[0] ||
            self.header[1] != MAGIC_NUMBER[1] ||
            self.header[2] != MAGIC_NUMBER[2] ||
            self.header[3] != MAGIC_NUMBER[3] {
                return Err(BrotliResult::ResultFailure);
            }
        let window_size = self.header[5] as usize;
        if window_size < 10 || window_size > 25 {
            return Err(BrotliResult::ResultFailure);
        }
        Ok(window_size)
    }

}

#[cfg(not(feature="billing"))]
fn print_decompression_result<AllocU8:Allocator<u8>>(_decompressor :&SelectedArithmeticDecoder!(AllocU8), _bytes_written: usize) {
   
}

#[cfg(feature="billing")]
fn print_decompression_result<AllocU8:Allocator<u8>>(decompressor :&SelectedArithmeticDecoder!(AllocU8), bytes_written: usize) {
   decompressor.print_compression_ratio(bytes_written);
}

pub enum DivansDecompressor<AllocU8:Allocator<u8>,
                            AllocCDF2:Allocator<probability::CDF2>,
                            AllocCDF16:Allocator<DefaultCDF16>> {
    Header(HeaderParser<AllocU8, AllocCDF2, AllocCDF16>),
    Decode(DivansCodec<SelectedArithmeticDecoder!(AllocU8), DecoderSpecialization, DefaultCDF16, AllocU8, AllocCDF2, AllocCDF16>, usize),
}
impl<AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> DivansDecompressor<AllocU8, AllocCDF2, AllocCDF16> {
    pub fn new(m8: AllocU8, mcdf2:AllocCDF2, mcdf16:AllocCDF16) -> Self {
        DivansDecompressor::Header(HeaderParser{header:[0u8;HEADER_LENGTH], read_offset:0, m8:Some(m8), mcdf2:Some(mcdf2), mcdf16:Some(mcdf16)})
    }
    pub fn finish_parsing_header(&mut self, window_size: usize) -> BrotliResult {
        if window_size < 10 {
            return BrotliResult::ResultFailure;
        }
        if window_size > 24 {
            return BrotliResult::ResultFailure;
        }
        let mut m8:AllocU8;
        let mcdf2:AllocCDF2;
        let mcdf16:AllocCDF16;
        match self {
            &mut DivansDecompressor::Header(ref mut header) => {
                m8 = match core::mem::replace(&mut header.m8, None) {
                    None => return BrotliResult::ResultFailure,
                    Some(m) => m,
                }
            },
            _ => return BrotliResult::ResultFailure,
        }
        match self {
            &mut DivansDecompressor::Header(ref mut header) => {
                mcdf2 = match core::mem::replace(&mut header.mcdf2, None) {
                    None => return BrotliResult::ResultFailure,
                    Some(m) => m,
                }
            },
            _ => return BrotliResult::ResultFailure,
        }
        match self {
            &mut DivansDecompressor::Header(ref mut header) => {
                mcdf16 = match core::mem::replace(&mut header.mcdf16, None) {
                    None => return BrotliResult::ResultFailure,
                    Some(m) => m,
                }
            },
            _ => return BrotliResult::ResultFailure,
        }
        //update this if you change the SelectedArithmeticDecoder macro
        let decoder = NewSelectedArithmeticDecoder!(AllocU8, &mut m8);
        core::mem::replace(self,
                           DivansDecompressor::Decode(DivansCodec::<SelectedArithmeticDecoder!(AllocU8),
                                                                    DecoderSpecialization,
                                                                    DefaultCDF16,
                                                                    AllocU8,
                                                                    AllocCDF2,
                                                                    AllocCDF16>::new(m8,
                                                                                     mcdf2,
                                                                                     mcdf16,
                                                                                     decoder,
                                                                                     DecoderSpecialization::new(),
                                                                                     window_size), 0));
        BrotliResult::ResultSuccess
    }
    pub fn free(self) -> (AllocU8, AllocCDF2, AllocCDF16) {
        match self {
            DivansDecompressor::Header(parser) => {
                (parser.m8.unwrap(),
                 parser.mcdf2.unwrap(),
                 parser.mcdf16.unwrap())
            },
            DivansDecompressor::Decode(decoder, bytes_encoded) => {
                print_decompression_result(&decoder.get_coder(), bytes_encoded);
                decoder.free()
            }
        }
    }
}

impl<AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> Decompressor for DivansDecompressor<AllocU8, AllocCDF2, AllocCDF16> {
    fn decode(&mut self,
              input:&[u8],
              input_offset:&mut usize,
              output:&mut [u8],
              output_offset: &mut usize) -> BrotliResult {
        let window_size: usize;

        match self  {
            &mut DivansDecompressor::Header(ref mut header_parser) => {
                let remaining = input.len() - *input_offset;
                let header_left = header_parser.header.len() - header_parser.read_offset;
                if remaining >= header_left {
                    header_parser.header[header_parser.read_offset..].clone_from_slice(
                        input.split_at(*input_offset).1.split_at(header_left).0);
                    *input_offset += header_left;
                    match header_parser.parse_header() {
                        Ok(wsize) => window_size = wsize,
                        Err(result) => return result,
                    }
                } else {
                    header_parser.header[(header_parser.read_offset)..
                                         (header_parser.read_offset+remaining)].clone_from_slice(
                        input.split_at(*input_offset).1);
                    *input_offset += remaining;
                    header_parser.read_offset += remaining;
                    return BrotliResult::NeedsMoreInput;
                }
            },
            &mut DivansDecompressor::Decode(ref mut divans_parser, ref mut bytes_encoded) => {
                let mut unused:usize = 0;
                let old_output_offset = *output_offset;
                let retval = divans_parser.encode_or_decode::<AllocU8::AllocatedMemory>(
                    input,
                    input_offset,
                    output,
                    output_offset,
                    &[],
                    &mut unused);
                *bytes_encoded += *output_offset - old_output_offset;
                return retval;
            },
        }
        self.finish_parsing_header(window_size);
        if *input_offset < input.len() {
            return self.decode(input, input_offset, output, output_offset);
        }
        BrotliResult::NeedsMoreInput
    }
}

