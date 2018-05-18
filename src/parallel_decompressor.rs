#![cfg(not(feature="no-stdlib"))]
use core;
use core::marker::PhantomData;
use core::hash::Hasher;
use ::interface;
use ::interface::{NewWithAllocator, Decompressor};
use ::DecoderSpecialization;
use ::codec;
use std::sync::{Arc, Mutex};
use divans_decompressor::HeaderParser;
use super::mux::{Mux,DevNull};
use codec::decoder::{DecoderResult, DivansDecoderCodec};
use threading::{ThreadToMainDemuxer, SerialWorker};
use multithreading::MultiWorker;

use ::interface::{DivansResult, DivansInputResult, ErrMsg};
use ::ArithmeticEncoderOrDecoder;
use ::alloc::{Allocator};
use std::thread;

pub struct DivansProcess<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                     AllocU8:Allocator<u8>,
                     AllocCDF16:Allocator<interface::DefaultCDF16>> {
    codec: Arc<Mutex<Option<codec::DivansCodec<DefaultDecoder,
                                         DecoderSpecialization,
                                         ThreadToMainDemuxer<AllocU8, MultiWorker<AllocU8>>,
                                         DevNull<AllocU8>,
                                         interface::DefaultCDF16,
                                         AllocU8,
                                         AllocCDF16>>>,
               >,
    worker: MultiWorker<AllocU8>,
    literal_decoder: Option<DivansDecoderCodec<interface::DefaultCDF16,
                                               AllocU8,
                                               AllocCDF16,
                                               DefaultDecoder,
                                               Mux<AllocU8>>>,
    bytes_encoded: usize,
}
pub enum DivansParallelDecompressor<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>+Send,
                            AllocU8:Allocator<u8>+Send,
                            AllocCDF16:Allocator<interface::DefaultCDF16>+Send> {
    Header(HeaderParser<AllocU8, AllocCDF16>),
    Decode(DivansProcess<DefaultDecoder,
                              AllocU8,
                              AllocCDF16>),
}

pub trait DivansParallelDecompressorFactory<
     AllocU8:Allocator<u8> + Send,
     AllocCDF16:Allocator<interface::DefaultCDF16> + Send> {
    type DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + Send;
    fn new(m8: AllocU8,
           mcdf16:AllocCDF16,
           skip_crc:bool) -> DivansParallelDecompressor<Self::DefaultDecoder, AllocU8, AllocCDF16> {
        DivansParallelDecompressor::Header(HeaderParser{header:[0u8;interface::HEADER_LENGTH], read_offset:0, m8:Some(m8), mcdf16:Some(mcdf16),
                                                skip_crc:skip_crc})
    }
}

impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability + Send + 'static,
                        AllocU8:Allocator<u8> + Send + 'static,
                        AllocCDF16:Allocator<interface::DefaultCDF16> + Send + 'static>
    DivansParallelDecompressor<DefaultDecoder, AllocU8, AllocCDF16>
    where <AllocU8 as Allocator<u8>>::AllocatedMemory: core::marker::Send,
          <AllocCDF16 as Allocator<interface::DefaultCDF16>>::AllocatedMemory: core::marker::Send,
{

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
            DivansParallelDecompressor::Header(ref mut header) => {
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
            DivansParallelDecompressor::Header(ref mut header) => {
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
                                             ThreadToMainDemuxer<AllocU8, MultiWorker<AllocU8>>,
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
        let multi_worker = (codec.demuxer().get_main_to_thread()).clone();
        let mut thread_codec = Arc::new(Mutex::new(Some(codec)));
        let worker_codec = thread_codec.clone();
        core::mem::replace(self,
                           DivansParallelDecompressor::Decode(
                               DivansProcess::<DefaultDecoder, AllocU8, AllocCDF16> {
                                   codec:worker_codec,
                                   literal_decoder:Some(main_thread_codec),
                                   bytes_encoded:0,
                                   worker: multi_worker,
                               }));
        thread::spawn(move || {
            let mut guard = thread_codec.lock().unwrap();
            if let Some(ref mut process_codec) = *guard {
                let mut unused_out = 0usize;
                let mut unused_in = 0usize;
                let mut unused = 0usize;
                loop {
                    match process_codec.encode_or_decode::<AllocU8::AllocatedMemory>(
                        &[],
                        &mut unused_in,
                        &mut [],
                        &mut unused_out,
                        &[],
                        &mut unused) {
                        DivansResult::Success => break, // DONE
                        DivansResult::Failure(e) => {
                            unimplemented!(); // HANDLE FAILURE BY TELLING MAIN THREAD
                    },
                        DivansResult::NeedsMoreInput => {
                            //eprintln!("W_RETRY_PULL");//unimplemented!(); // we should block here--- maybe this is an error
                        },
                        DivansResult::NeedsMoreOutput => {}, // lets make room for more output
                    }
                }
            } else {
                panic!("Thread started with None-process_codec")
            }
        });
        DivansResult::Success
    }
    pub fn free_ref(&mut self) {
        if let DivansParallelDecompressor::Decode(ref mut process) = *self {
            if let Some(ref mut codec) = *process.codec.lock().unwrap() {
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
            DivansParallelDecompressor::Header(parser) => {
                (parser.m8.unwrap(),
                 parser.mcdf16.unwrap())
            },
            DivansParallelDecompressor::Decode(mut process) => {
                use codec::NUM_ARITHMETIC_CODERS;
                if let Some(mut codec) = core::mem::replace(&mut *process.codec.lock().unwrap(), None) {
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

impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability+Send+'static,
     AllocU8:Allocator<u8>+Send+'static,
     AllocCDF16:Allocator<interface::DefaultCDF16> +Send+'static> Decompressor for DivansParallelDecompressor<DefaultDecoder,
                                                                                        AllocU8,
                                                                                                              AllocCDF16>
    where <AllocU8 as Allocator<u8>>::AllocatedMemory: core::marker::Send,
          <AllocCDF16 as Allocator<interface::DefaultCDF16>>::AllocatedMemory: core::marker::Send,
    {
    fn decode(&mut self,
              input:&[u8],
              input_offset:&mut usize,
              output:&mut [u8],
              output_offset: &mut usize) -> DivansResult {
        let window_size: usize;

        match *self  {
            DivansParallelDecompressor::Header(ref mut header_parser) => {
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
            DivansParallelDecompressor::Decode(ref mut process) => {
                let old_output_offset = *output_offset;
                if let Some(literal_decoder) =  process.literal_decoder.as_mut() {
                    loop {
                        match literal_decoder.decode_process_input(&mut process.worker,
                                                                   input,
                                                                   input_offset) {
                            DivansInputResult::Success => {},
                            need_something => return DivansResult::from(need_something),
                        }
                        if literal_decoder.commands_or_data_to_receive() {
                            break; // we have successfully delivered a buffer to our worker and then can, at worst pull the result
                        }
                    }
                    let retval = literal_decoder.decode_process_output(
                        &mut process.worker,
                        output,
                        output_offset);
                    process.bytes_encoded += *output_offset - old_output_offset;
                    match retval {
                        DecoderResult::Processed(divans_retval) => {
                            return divans_retval;
                        },
                        DecoderResult::Yield => {
                            unreachable!(); // we are not marked cooperative
                        },
                    }
                } else {
                    return DivansResult::Failure(ErrMsg::DecodingDecoderAlreadyFreed);
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

pub struct DivansParallelDecompressorFactoryStruct
    <AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> {
    p1: PhantomData<AllocU8>,
    p2: PhantomData<AllocCDF16>,
}

impl<AllocU8:Allocator<u8>+Send,
     AllocCDF16:Allocator<interface::DefaultCDF16>+Send> DivansParallelDecompressorFactory<AllocU8,
                                                                                   AllocCDF16>
    for DivansParallelDecompressorFactoryStruct<AllocU8, AllocCDF16> {
     type DefaultDecoder = DefaultDecoderType!();
}
