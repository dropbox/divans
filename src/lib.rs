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
mod debug_encoder;
mod encoder;
mod cmd_to_raw;
mod codec;
mod cmd_to_divans;
mod divans_to_raw;
mod billing;
pub use brotli_decompressor::{BrotliResult};
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use interface::{Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop};
pub use cmd_to_raw::DivansRecodeState;
pub use codec::CMD_BUFFER_SIZE;
pub use divans_to_raw::DecoderSpecialization;
pub use cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec};

const HEADER_LENGTH: usize = 16;
const MAGIC_NUMBER:[u8;4] = [0xff, 0xe5,0x8c, 0x9f];

pub type DefaultArithmeticEncoder = debug_encoder::DebugEncoder;
pub type DefaultArithmeticDecoder = debug_encoder::DebugDecoder;

pub type DefaultCDF16 = probability::FrequentistCDF16;


#[cfg(not(feature="billing"))]
pub type SelectedArithmeticEncoder = DefaultArithmeticEncoder;
#[cfg(not(feature="billing"))]
pub type SelectedArithmeticDecoder = DefaultArithmeticDecoder;


#[cfg(feature="billing")]
pub type SelectedArithmeticEncoder = billing::BillingArithmeticCoder<DefaultArithmeticEncoder>;
#[cfg(feature="billing")]
pub type SelectedArithmeticDecoder = billing::BillingArithmeticCoder<DefaultArithmeticDecoder>;


pub struct DivansCompressor<AllocU8:Allocator<u8> > {
    codec: DivansCodec<SelectedArithmeticEncoder, EncoderSpecialization, DefaultCDF16, AllocU8>,
    header_progress: usize,
    window_size: u8,
}
impl<AllocU8:Allocator<u8> > DivansCompressor<AllocU8> {
    pub fn new(m8: AllocU8, mut window_size: usize) -> Self {
        if window_size < 10 {
            window_size = 10;
        }
        if window_size > 24 {
            window_size = 24;
        }
        DivansCompressor::<AllocU8> {
            codec:DivansCodec::<SelectedArithmeticEncoder, EncoderSpecialization, DefaultCDF16, AllocU8>::new(
                m8,
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

impl<AllocU8:Allocator<u8>> DivansCompressor<AllocU8>   {
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

impl<AllocU8:Allocator<u8>> Compressor for DivansCompressor<AllocU8>   {
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


pub struct HeaderParser<AllocU8:Allocator<u8>> {
    header:[u8;HEADER_LENGTH],
    read_offset: usize,
    m8: Option<AllocU8>,
}
impl<AllocU8:Allocator<u8>> HeaderParser<AllocU8> {
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
fn print_decompression_result(_decompressor :&SelectedArithmeticDecoder, _bytes_written: usize) {
   
}

#[cfg(feature="billing")]
fn print_decompression_result(decompressor :&SelectedArithmeticDecoder, bytes_written: usize) {
   decompressor.print_compression_ratio(bytes_written);
}

pub enum DivansDecompressor<AllocU8:Allocator<u8> > {
    Header(HeaderParser<AllocU8>),
    Decode(DivansCodec<SelectedArithmeticDecoder, DecoderSpecialization, DefaultCDF16, AllocU8>, usize),
}
impl<AllocU8:Allocator<u8> > DivansDecompressor<AllocU8> {
    pub fn new(m8: AllocU8) -> Self {
        DivansDecompressor::Header(HeaderParser{header:[0u8;HEADER_LENGTH], read_offset:0, m8:Some(m8)})
    }
    pub fn finish_parsing_header(&mut self, window_size: usize) -> BrotliResult {
        if window_size < 10 {
            return BrotliResult::ResultFailure;
        }
        if window_size > 24 {
            return BrotliResult::ResultFailure;
        }
        let m8:AllocU8;
        match self {
            &mut DivansDecompressor::Header(ref mut header) => {
                m8 = match core::mem::replace(&mut header.m8, None) {
                    None => return BrotliResult::ResultFailure,
                    Some(m) => m,
                }
            },
            _ => return BrotliResult::ResultFailure,
        }
        core::mem::replace(self,
                           DivansDecompressor::Decode(DivansCodec::<SelectedArithmeticDecoder,
                                                                    DecoderSpecialization,
                                                                    DefaultCDF16,
                                                                    AllocU8>::new(m8,
                                                                                  DecoderSpecialization::new(),
                                                                                  window_size), 0));
        BrotliResult::ResultSuccess
    }
    pub fn free(self) ->AllocU8 {
        match self {
            DivansDecompressor::Header(parser) => {
                parser.m8.unwrap()
            },
            DivansDecompressor::Decode(decoder, bytes_encoded) => {
                print_decompression_result(&decoder.get_coder(), bytes_encoded);
                decoder.free()
            }
        }
    }
}

impl<AllocU8:Allocator<u8>> Decompressor for DivansDecompressor<AllocU8> {
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

