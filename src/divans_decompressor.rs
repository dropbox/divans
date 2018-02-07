use core;
use core::marker::PhantomData;
use ::probability;
use ::interface;
use ::interface::{NewWithAllocator, Decompressor};
use ::DecoderSpecialization;
use ::codec;

use ::BrotliResult;
use ::ArithmeticEncoderOrDecoder;
use ::alloc::{Allocator,  SliceWrapperMut};

pub struct HeaderParser<AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<probability::CDF2>,
                        AllocCDF16:Allocator<interface::DefaultCDF16>> {
    header:[u8;interface::HEADER_LENGTH],
    read_offset: usize,
    m8: Option<AllocU8>,
    mcdf2: Option<AllocCDF2>,
    mcdf16: Option<AllocCDF16>,
    custom_dict: AllocU8::AllocatedMemory,
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
    Decode(codec::DivansCodec<DefaultDecoder,
           DecoderSpecialization,
           interface::DefaultCDF16,
           AllocU8,
           AllocCDF2,
           AllocCDF16>,
           usize),
}

pub fn alloc_dict<AllocU8:Allocator<u8>>(m8: &mut AllocU8,
                                     ring_buffer_hint: Option<usize>,
                                     custom_dict: &[u8]) -> AllocU8::AllocatedMemory{
    if custom_dict.len() == 0 {
        return AllocU8::AllocatedMemory::default();
    }
    let mut len = custom_dict.len();
    if custom_dict.len() > (1 << 24) {
        len = 1 << 24;
    }
    len -= 1;
    len |= len >> 1; // round up to power of two
    len |= len >> 2;
    len |= len >> 3;
    len |= len >> 8;
    len |= len >> 16;
    len += 1;
    if let Some(rbh) = ring_buffer_hint {
        len = rbh;
    }
    let mut ret = m8.alloc_cell(len);
    if custom_dict.len() > len {
        ret.slice_mut().clone_from_slice(&custom_dict.split_at(custom_dict.len() - len).1);
    } else {
        ret.slice_mut().split_at_mut(len - custom_dict.len()).1.clone_from_slice(&custom_dict[(custom_dict.len() - len)..]);
    }
    ret
}

pub trait DivansDecompressorFactory<
     AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> {
    type DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>;
    fn new(mut m8: AllocU8, mcdf2:AllocCDF2, mcdf16:AllocCDF16, dict: &[u8]) -> DivansDecompressor<Self::DefaultDecoder, AllocU8, AllocCDF2, AllocCDF16> {
        let custom_dict = alloc_dict(&mut m8, None, dict);
        DivansDecompressor::Header(HeaderParser{header:[0u8;interface::HEADER_LENGTH],
                                                read_offset:0,
                                                m8:Some(m8), mcdf2:Some(mcdf2), mcdf16:Some(mcdf16),
                                                custom_dict: custom_dict,
        })
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
        let custom_dict: AllocU8::AllocatedMemory;
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
                custom_dict = core::mem::replace(&mut header.custom_dict, AllocU8::AllocatedMemory::default());
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
                           DivansDecompressor::Decode(
                               codec::DivansCodec::<DefaultDecoder,
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
                                                                     custom_dict,
                                                                     0,
                                                                     None,
                                                                     None,
                                                                     true,
                                                                     codec::StrideSelection::UseBrotliRec), 0));
        BrotliResult::ResultSuccess
    }
    pub fn free_ref(&mut self) {
        if let DivansDecompressor::Decode(ref mut decoder, _bytes_encoded) = *self {
            decoder.free_ref();
        }
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
