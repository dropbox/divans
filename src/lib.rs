#![no_std]
#[cfg(test)]
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
pub use brotli_decompressor::{BrotliResult};
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use interface::{Command, Decompressor, Compressor, LiteralCommand, CopyCommand, DictCommand};
pub use cmd_to_raw::DivansRecodeState;
pub use codec::CMD_BUFFER_SIZE;
pub use divans_to_raw::DecoderSpecialization;
pub use cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec};

pub type DefaultArithmeticEncoder = debug_encoder::DebugEncoder;
pub type DefaultArithmeticDecoder = debug_encoder::DebugDecoder;

pub struct DivansCompressor<AllocU8:Allocator<u8> > {
    _codec: DivansCodec<DefaultArithmeticEncoder, EncoderSpecialization, AllocU8>,
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
            _codec:DivansCodec::<DefaultArithmeticEncoder, EncoderSpecialization, AllocU8>::new(
                m8,
                EncoderSpecialization::new(),
                window_size,
            ),
        }
    }
}

const HEADER_LENGTH: usize = 16;

pub struct HeaderParser<AllocU8:Allocator<u8>> {
    header:[u8;HEADER_LENGTH],
    read_offset: usize,
    m8: Option<AllocU8>,
}

pub enum DivansDecompressor<AllocU8:Allocator<u8> > {
    Header(HeaderParser<AllocU8>),
    Decode(DivansCodec<DefaultArithmeticDecoder, DecoderSpecialization, AllocU8>),
}
impl<AllocU8:Allocator<u8> > DivansDecompressor<AllocU8> {
    pub fn new(m8: AllocU8) -> Self {
        DivansDecompressor::Header(HeaderParser{header:[0u8;HEADER_LENGTH], read_offset:0, m8:Some(m8)})
    }
    pub fn parsed_header(&mut self, window_size: usize) -> BrotliResult {
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
                           DivansDecompressor::Decode(DivansCodec::<DefaultArithmeticDecoder,
                                                                    DecoderSpecialization,
                                                                    AllocU8>::new(m8,
                                                                                  DecoderSpecialization::new(),
                                                                                  window_size)));
        BrotliResult::ResultSuccess
    }
}

/*
    pub fn decode(&mut self,
                  input:&[u8],
                  input_offset: &mut usize,
                  output: &mut [u8],
                  output_offset: &mut usize) -> BrotliResult {
        let input_len = input.len();
        let output_len = output.len();
        let mut needs_input = false;
        loop {
            if self.buffer_size == self.buffer_offset {
                self.buffer_size = 0;
                self.buffer_offset = 0;
            }
            if *input_offset < input_len && self.buffer_size < self.buffer.len() && self.decode_complete == false && needs_input == false{
                match self.decoder.decode(input,
                                     input_offset,
                                     &mut self.buffer,
                                     &mut self.buffer_size) {
                    BrotliResult::NeedsMoreInput => {
                        needs_input = true;
                    },
                    BrotliResult::NeedsMoreOutput => {
                    },
                    BrotliResult::ResultFailure => {
                        return BrotliResult::ResultFailure;
                    },
                    BrotliResult::ResultSuccess => {
                        self.decode_complete = true;
                    }
                }
            }
            if self.buffer_offset < self.buffer_size && *output_offset < output_len {
                match self.recoder.recode(self.buffer.split_at(self.buffer_size).0,
                                     &mut self.buffer_offset,
                                     output,
                                     output_offset) {
                    BrotliResult::NeedsMoreInput => {
                        assert_eq!(self.buffer_size, self.buffer_offset);
                        if needs_input {
                            return BrotliResult::NeedsMoreInput;
                        }
                        if self.decode_complete {
                            return BrotliResult::ResultSuccess;
                        }
                    },
                    BrotliResult::NeedsMoreOutput => {
                        return BrotliResult::NeedsMoreOutput;
                    },
                    BrotliResult::ResultFailure => {
                        return BrotliResult::ResultFailure;
                    },
                    BrotliResult::ResultSuccess => {
                        if self.decode_complete {
                            return BrotliResult::ResultSuccess;
                        }
                    },
                }
            }
        }
    }
}


*/
