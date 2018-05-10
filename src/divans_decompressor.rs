use core;
use core::marker::PhantomData;
use core::hash::Hasher;
use ::probability;
use ::interface;
use ::interface::{NewWithAllocator, Decompressor};
use ::DecoderSpecialization;
use ::codec;
use super::mux::{Mux,DevNull};
use ::interface::DivansResult;
use ::interface::ErrMsg;
use ::ArithmeticEncoderOrDecoder;
use ::alloc::{Allocator};

pub struct HeaderParser<AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<probability::CDF2>,
                        AllocCDF16:Allocator<interface::DefaultCDF16>> {
    header:[u8;interface::HEADER_LENGTH],
    read_offset: usize,
    m8: Option<AllocU8>,
    mcdf2: Option<AllocCDF2>,
    mcdf16: Option<AllocCDF16>,
    skip_crc: bool,
}
impl<AllocU8:Allocator<u8>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>>HeaderParser<AllocU8, AllocCDF2, AllocCDF16> {
    pub fn parse_header(&mut self)->Result<usize, DivansResult>{
        if self.header[0] != interface::MAGIC_NUMBER[0] ||
            self.header[1] != interface::MAGIC_NUMBER[1] {
                return Err(DivansResult::Failure(ErrMsg::MagicNumberWrongA(self.header[0], self.header[1])));
        }
        if self.header[2] != interface::MAGIC_NUMBER[2] ||
            self.header[3] != interface::MAGIC_NUMBER[3] {
                return Err(DivansResult::Failure(ErrMsg::MagicNumberWrongB(self.header[2], self.header[3])));
        }
        let window_size = self.header[5] as usize;
        if window_size < 10 || window_size >= 25 {
            return Err(DivansResult::Failure(ErrMsg::BadWindowSize(window_size as u8)));
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
                              Mux<AllocU8>,
                              DevNull<AllocU8>,
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
    fn new(m8: AllocU8,
           mcdf2:AllocCDF2,
           mcdf16:AllocCDF16,
           skip_crc:bool) -> DivansDecompressor<Self::DefaultDecoder, AllocU8, AllocCDF2, AllocCDF16> {
        DivansDecompressor::Header(HeaderParser{header:[0u8;interface::HEADER_LENGTH], read_offset:0, m8:Some(m8), mcdf2:Some(mcdf2), mcdf16:Some(mcdf16),
                                                skip_crc:skip_crc})
    }
}

impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
                        AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<probability::CDF2>,
                        AllocCDF16:Allocator<interface::DefaultCDF16>>
    DivansDecompressor<DefaultDecoder, AllocU8, AllocCDF2, AllocCDF16> {

    fn finish_parsing_header(&mut self, window_size: usize) -> DivansResult {
        if window_size < 10 {
            return DivansResult::Failure(ErrMsg::BadWindowSize(window_size as u8));
        }
        if window_size > 24 {
            return DivansResult::Failure(ErrMsg::BadWindowSize(window_size as u8));
        }
        let mut m8:AllocU8;
        let mcdf2:AllocCDF2;
        let mcdf16:AllocCDF16;
        let raw_header:[u8; interface::HEADER_LENGTH];
        let skip_crc:bool;
        match *self {
            DivansDecompressor::Header(ref mut header) => {
                m8 = match core::mem::replace(&mut header.m8, None) {
                    None => return DivansResult::Failure(ErrMsg::MissingAllocator(8)),
                    Some(m) => m,
                };
                raw_header = header.header;
                skip_crc = header.skip_crc;
            },
            _ => return DivansResult::Failure(ErrMsg::WrongInternalDecoderState),
        }
        match *self {
            DivansDecompressor::Header(ref mut header) => {
                mcdf2 = match core::mem::replace(&mut header.mcdf2, None) {
                    None => return DivansResult::Failure(ErrMsg::MissingAllocator(2)),
                    Some(m) => m,
                }
            },
            _ => return DivansResult::Failure(ErrMsg::WrongInternalDecoderState),
        }
        match *self {
            DivansDecompressor::Header(ref mut header) => {
                mcdf16 = match core::mem::replace(&mut header.mcdf16, None) {
                    None => return DivansResult::Failure(ErrMsg::MissingAllocator(16)),
                    Some(m) => m,
                }
            },
            _ => return DivansResult::Failure(ErrMsg::WrongInternalDecoderState),
        }
        //update this if you change the SelectedArithmeticDecoder macro
        let cmd_decoder = DefaultDecoder::new(&mut m8);
        let lit_decoder = DefaultDecoder::new(&mut m8);
        let mut codec = codec::DivansCodec::<DefaultDecoder,
                                             DecoderSpecialization,
                                             Mux<AllocU8>,
                                             DevNull<AllocU8>,
                                             interface::DefaultCDF16,
                                             AllocU8,
                                             AllocCDF2,
                                             AllocCDF16>::new(m8,
                                                              mcdf2,
                                                              mcdf16,
                                                              cmd_decoder,
                                                              lit_decoder,
                                                              DecoderSpecialization::new(),
                                                              window_size,
                                                              0,
                                                              None,
                                                              None,
                                                          true,
                                                              codec::StrideSelection::UseBrotliRec,
                                                              skip_crc);
        if !skip_crc {
            codec.get_crc().write(&raw_header[..]);
        }
        core::mem::replace(self,
                           DivansDecompressor::Decode(
                               codec, 0));
        DivansResult::Success
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
                use codec::NUM_ARITHMETIC_CODERS;
                for index in 0..NUM_ARITHMETIC_CODERS {
                    decoder.get_coder(index as u8).debug_print(bytes_encoded);
                }
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
              output_offset: &mut usize) -> DivansResult {
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
                    return DivansResult::NeedsMoreInput;
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
        DivansResult::NeedsMoreInput
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
