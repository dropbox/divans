use core;
use core::marker::PhantomData;
use core::hash::Hasher;
use ::interface;
use ::interface::{NewWithAllocator, Decompressor};
use ::DecoderSpecialization;
use ::codec;
use super::mux::{Mux,DevNull};
use codec::decoder::{DecoderResult, DivansDecoderCodec};
use threading::{ThreadToMainDemuxer, SerialWorker};


use ::interface::{DivansResult, DivansInputResult, ErrMsg};
use ::ArithmeticEncoderOrDecoder;
use ::alloc::{Allocator};

pub struct HeaderParser<AllocU8:Allocator<u8>,
                        AllocCDF16:Allocator<interface::DefaultCDF16>> {
    header:[u8;interface::HEADER_LENGTH],
    read_offset: usize,
    m8: Option<AllocU8>,
    mcdf16: Option<AllocCDF16>,
    skip_crc: bool,
}
impl<AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>>HeaderParser<AllocU8, AllocCDF16> {
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

pub struct DivansProcess<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                     AllocU8:Allocator<u8>,
                     AllocCDF16:Allocator<interface::DefaultCDF16>> {
    codec: Option<codec::DivansCodec<DefaultDecoder,
                              DecoderSpecialization,
                              ThreadToMainDemuxer<AllocU8, SerialWorker<AllocU8>>,
                              DevNull<AllocU8>,
                              interface::DefaultCDF16,
                              AllocU8,
                                     AllocCDF16>>,
    literal_decoder: Option<DivansDecoderCodec<interface::DefaultCDF16,
                                               AllocU8,
                                               AllocCDF16,
                                               DefaultDecoder,
                                               Mux<AllocU8>>>,
    bytes_encoded: usize,
}
pub enum DivansDecompressor<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                            AllocU8:Allocator<u8>,
                            AllocCDF16:Allocator<interface::DefaultCDF16>> {
    Header(HeaderParser<AllocU8, AllocCDF16>),
    Decode(DivansProcess<DefaultDecoder,
                              AllocU8,
                              AllocCDF16>),
}

pub trait DivansDecompressorFactory<
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> {
    type DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>;
    fn new(m8: AllocU8,
           mcdf16:AllocCDF16,
           skip_crc:bool) -> DivansDecompressor<Self::DefaultDecoder, AllocU8, AllocCDF16> {
        DivansDecompressor::Header(HeaderParser{header:[0u8;interface::HEADER_LENGTH], read_offset:0, m8:Some(m8), mcdf16:Some(mcdf16),
                                                skip_crc:skip_crc})
    }
}

impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
                        AllocU8:Allocator<u8>,
                        AllocCDF16:Allocator<interface::DefaultCDF16>>
    DivansDecompressor<DefaultDecoder, AllocU8, AllocCDF16> {

    fn finish_parsing_header(&mut self, window_size: usize) -> DivansResult {
        if window_size < 10 {
            return DivansResult::Failure(ErrMsg::BadWindowSize(window_size as u8));
        }
        if window_size > 24 {
            return DivansResult::Failure(ErrMsg::BadWindowSize(window_size as u8));
        }
        let mut m8:AllocU8;
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
                                             ThreadToMainDemuxer<AllocU8, SerialWorker<AllocU8>>,
                                             DevNull<AllocU8>,
                                             interface::DefaultCDF16,
                                             AllocU8,
                                             AllocCDF16>::new(m8,
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
        let main_thread_codec = codec.fork();
        assert_eq!(*codec.get_crc(), main_thread_codec.crc);
        core::mem::replace(self,
                           DivansDecompressor::Decode(
                               DivansProcess::<DefaultDecoder, AllocU8, AllocCDF16> {
                                   codec:Some(codec),
                                   literal_decoder:Some(main_thread_codec),
                                   bytes_encoded:0,
                               }));
        DivansResult::Success
    }
    pub fn free_ref(&mut self) {
        if let DivansDecompressor::Decode(ref mut process) = *self {
            if let Some(ref mut codec) = process.codec {
                let lit_decoder = core::mem::replace(&mut process.literal_decoder, None);
                if let Some(ld) = lit_decoder {
                    codec.join(ld);
                }
                codec.free_ref();
            }
        }
    }
    pub fn free(self) -> (AllocU8, AllocCDF16) {
        match self {
            DivansDecompressor::Header(parser) => {
                (parser.m8.unwrap(),
                 parser.mcdf16.unwrap())
            },
            DivansDecompressor::Decode(mut process) => {
                use codec::NUM_ARITHMETIC_CODERS;
                if let Some(mut codec) = core::mem::replace(&mut process.codec, None) {
                    let lit_decoder = core::mem::replace(&mut process.literal_decoder, None);
                    if let Some(ld) = lit_decoder {
                        codec.join(ld);
                    }
                    for index in 0..NUM_ARITHMETIC_CODERS {
                        codec.get_coder(index as u8).debug_print(process.bytes_encoded);
                    }
                    codec.free()
                } else {
                    panic!("Trying to free unjoined decoder"); //FIXME: this does not seem ergonomic
                }
            }
        }
    }
}

impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> Decompressor for DivansDecompressor<DefaultDecoder,
                                                                                        AllocU8,
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
            DivansDecompressor::Decode(ref mut process) => {
                let mut unused:usize = 0;
                let old_output_offset = *output_offset;
                loop {
                    match process.literal_decoder.as_mut().unwrap().decode_process_input(process.codec.as_mut().unwrap().demuxer().get_main_to_thread(),
                                                                                         input,
                                                                                         input_offset) {
                        DivansInputResult::Success => {},
                        need_something => return DivansResult::from(need_something),
                    }
                    let mut unused_out = 0usize;
                    let mut unused_in = 0usize;
                    match process.codec.as_mut().unwrap().encode_or_decode::<AllocU8::AllocatedMemory>(
                        &[],
                        &mut unused_in,
                        &mut [],
                        &mut unused_out,
                        &[],
                        &mut unused) {
                        DivansResult::Success => {},
                        DivansResult::Failure(e) => return DivansResult::Failure(e),
                        DivansResult::NeedsMoreInput => {
                            if process.literal_decoder.as_mut().unwrap().outstanding_buffer_count == 0 {
                                return DivansResult::NeedsMoreInput;
                            } else {
                                // we can fall through here because if outstanding_buffer_count != 0 then
                                // the worker either consumed the buffer and returned a command or returned the buffer (a command)

                            }
                        },
                        DivansResult::NeedsMoreOutput => {}, // lets make room for more output
                    }
                    let retval = process.literal_decoder.as_mut().unwrap().decode_process_output(
                        process.codec.as_mut().unwrap().demuxer().get_main_to_thread(),
                        output,
                        output_offset);
                    process.bytes_encoded += *output_offset - old_output_offset;
                    match retval {
                        DecoderResult::Processed(divans_retval) => {
                            return divans_retval;
                        },
                        DecoderResult::Yield => {
                        },
                    }
                }
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
     AllocCDF16:Allocator<interface::DefaultCDF16>> {
    p1: PhantomData<AllocU8>,
    p2: PhantomData<AllocCDF16>,
}

impl<AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> DivansDecompressorFactory<AllocU8,
                                                                              AllocCDF16>
    for DivansDecompressorFactoryStruct<AllocU8, AllocCDF16> {
     type DefaultDecoder = DefaultDecoderType!();
}
