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
#![cfg_attr(feature="benchmark", feature(test))]
#![feature(platform_intrinsics)]
#![no_std]
#[macro_use]

#[cfg(not(test))]
#[cfg(feature="billing")]
#[macro_use]
extern crate std;
#[cfg(feature="simd")]
extern crate stdsimd;

#[cfg(test)]
#[macro_use]
extern crate std;

extern crate alloc_no_stdlib as alloc;
extern crate brotli;
pub mod resizable_byte_buffer;
pub mod interface;
pub mod slice_util;
pub mod alloc_util;
mod probability;
#[macro_use]
mod priors;
#[macro_use]
mod encoder;
mod debug_encoder;
mod cmd_to_raw;
mod raw_to_cmd;
mod codec;
mod cmd_to_divans;
mod divans_to_raw;
#[macro_use]
mod billing;
pub mod test_helper;
mod test_ans;
mod ans;
mod brotli_ir_gen;
mod divans_compressor;
pub mod constants;
pub use brotli::BrotliResult;
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use interface::{DivansCompressorFactory, BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType,
                    DefaultCDF16};

pub use brotli_ir_gen::{BrotliDivansHybridCompressor,BrotliDivansHybridCompressorFactory};
pub use cmd_to_raw::DivansRecodeState;
pub use codec::CMD_BUFFER_SIZE;
pub use divans_to_raw::DecoderSpecialization;
pub use cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec, StrideSelection};
pub use divans_compressor::{DivansCompressor, DivansCompressorFactoryStruct};
use core::marker::PhantomData;


pub use probability::Speed;


pub use probability::CDF2;



pub struct HeaderParser<AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<probability::CDF2>,
                        AllocCDF16:Allocator<interface::DefaultCDF16>> {
    header:[u8;interface::HEADER_LENGTH],
    read_offset: usize,
    m8: Option<AllocU8>,
    mcdf2: Option<AllocCDF2>,
    mcdf16: Option<AllocCDF16>,
}
impl<AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>>HeaderParser<AllocU8, AllocCDF2, AllocCDF16> {
    pub fn parse_header(&mut self)->Result<usize, BrotliResult>{
        if self.header[0] != interface::MAGIC_NUMBER[0] ||
            self.header[1] != interface::MAGIC_NUMBER[1] ||
            self.header[2] != interface::MAGIC_NUMBER[2] ||
            self.header[3] != interface::MAGIC_NUMBER[3] {
                return Err(BrotliResult::ResultFailure);
            }
        let window_size = self.header[5] as usize;
        if window_size < 10 || window_size > 25 {
            return Err(BrotliResult::ResultFailure);
        }
        Ok(window_size)
    }

}

pub enum DivansDecompressor<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                            AllocU8:Allocator<u8>,
                            AllocCDF2:Allocator<probability::CDF2>,
                            AllocCDF16:Allocator<interface::DefaultCDF16>> {
    Header(HeaderParser<AllocU8, AllocCDF2, AllocCDF16>),
    Decode(DivansCodec<DefaultDecoder,
           DecoderSpecialization,
           interface::DefaultCDF16,
           AllocU8,
           AllocCDF2,
           AllocCDF16>,
           usize),
}

pub trait DivansDecompressorFactory<
     AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> {
     type DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>;
    fn new(m8: AllocU8, mcdf2:AllocCDF2, mcdf16:AllocCDF16) -> DivansDecompressor<Self::DefaultDecoder, AllocU8, AllocCDF2, AllocCDF16> {
        DivansDecompressor::Header(HeaderParser{header:[0u8;interface::HEADER_LENGTH], read_offset:0, m8:Some(m8), mcdf2:Some(mcdf2), mcdf16:Some(mcdf16)})
    }
}

impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
                        AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<probability::CDF2>,
                        AllocCDF16:Allocator<interface::DefaultCDF16>>
    DivansDecompressor<DefaultDecoder, AllocU8, AllocCDF2, AllocCDF16> {

    fn finish_parsing_header(&mut self, window_size: usize) -> BrotliResult {
        if window_size < 10 {
            return BrotliResult::ResultFailure;
        }
        if window_size > 24 {
            return BrotliResult::ResultFailure;
        }
        let mut m8:AllocU8;
        let mcdf2:AllocCDF2;
        let mcdf16:AllocCDF16;
        match *self {
            DivansDecompressor::Header(ref mut header) => {
                m8 = match core::mem::replace(&mut header.m8, None) {
                    None => return BrotliResult::ResultFailure,
                    Some(m) => m,
                }
            },
            _ => return BrotliResult::ResultFailure,
        }
        match *self {
            DivansDecompressor::Header(ref mut header) => {
                mcdf2 = match core::mem::replace(&mut header.mcdf2, None) {
                    None => return BrotliResult::ResultFailure,
                    Some(m) => m,
                }
            },
            _ => return BrotliResult::ResultFailure,
        }
        match *self {
            DivansDecompressor::Header(ref mut header) => {
                mcdf16 = match core::mem::replace(&mut header.mcdf16, None) {
                    None => return BrotliResult::ResultFailure,
                    Some(m) => m,
                }
            },
            _ => return BrotliResult::ResultFailure,
        }
        //update this if you change the SelectedArithmeticDecoder macro
        let decoder = DefaultDecoder::new(&mut m8);
        core::mem::replace(self,
                           DivansDecompressor::Decode(DivansCodec::<DefaultDecoder,
                                                                    DecoderSpecialization,
                                                                    interface::DefaultCDF16,
                                                                    AllocU8,
                                                                    AllocCDF2,
                                                                    AllocCDF16>::new(m8,
                                                                                     mcdf2,
                                                                                     mcdf16,
                                                                                     decoder,
                                                                                     DecoderSpecialization::new(),
                                                                                     window_size,
                                                                                     0,
                                                                                     None,
                                                                                     true,
                                                                                     codec::StrideSelection::UseBrotliRec), 0));
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
                decoder.get_coder().debug_print(bytes_encoded);
                decoder.free()
            }
        }
    }
}

impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
     AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> Decompressor for DivansDecompressor<DefaultDecoder,
                                                                                        AllocU8,
                                                                                        AllocCDF2,
                                                                                        AllocCDF16> {
    fn decode(&mut self,
              input:&[u8],
              input_offset:&mut usize,
              output:&mut [u8],
              output_offset: &mut usize) -> BrotliResult {
        let window_size: usize;

        match *self  {
            DivansDecompressor::Header(ref mut header_parser) => {
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
            DivansDecompressor::Decode(ref mut divans_parser, ref mut bytes_encoded) => {
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

pub struct DivansDecompressorFactoryStruct
    <AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> {
    p1: PhantomData<AllocU8>,
    p2: PhantomData<AllocCDF2>,
    p3: PhantomData<AllocCDF16>,
}

impl<AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> DivansDecompressorFactory<AllocU8,
                                                                              AllocCDF2,
                                                                              AllocCDF16>
    for DivansDecompressorFactoryStruct<AllocU8, AllocCDF2, AllocCDF16> {
     type DefaultDecoder = DefaultDecoderType!();
}
