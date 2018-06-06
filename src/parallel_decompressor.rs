#![cfg(not(feature="no-stdlib"))]
use core;
use core::hash::Hasher;
use ::interface;
use ::interface::{NewWithAllocator};
use ::DecoderSpecialization;
use ::codec;
use std::sync::{Arc, Mutex};
use divans_decompressor::HeaderParser;
use super::mux::{Mux,DevNull};
use codec::decoder::{DecoderResult, DivansDecoderCodec};
use threading::{ThreadToMainDemuxer};
use multithreading::{BufferedMultiWorker, MultiWorker};

use ::interface::{DivansResult, DivansInputResult, ErrMsg};
use ::ArithmeticEncoderOrDecoder;
use ::alloc::{Allocator};
use std::thread;
use super::divans_decompressor::StaticCommand;

pub struct ParallelDivansProcess<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                                 AllocU8:Allocator<u8>,
                                 AllocCDF16:Allocator<interface::DefaultCDF16>,
                                 AllocCommand:Allocator<StaticCommand>> {
    codec: Arc<Mutex<Option<codec::DivansCodec<DefaultDecoder,
                                         DecoderSpecialization,
                                         ThreadToMainDemuxer<AllocU8, BufferedMultiWorker<AllocU8, AllocCommand>>,
                                         DevNull<AllocU8>,
                                         interface::DefaultCDF16,
                                         AllocU8,
                                         AllocCDF16>>>,
               >,
    worker: MultiWorker<AllocU8, AllocCommand>,
    literal_decoder: Option<DivansDecoderCodec<interface::DefaultCDF16,
                                               AllocU8,
                                               AllocCDF16,
                                               AllocCommand,
                                               DefaultDecoder,
                                               Mux<AllocU8>>>,
    bytes_encoded: usize,
    mcommand: AllocCommand,
}


impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability + Send + 'static,
     AllocU8:Allocator<u8> + Send + 'static,
     AllocCDF16:Allocator<interface::DefaultCDF16> + Send + 'static,
     AllocCommand:Allocator<StaticCommand> + Send + 'static>
    ParallelDivansProcess<DefaultDecoder, AllocU8, AllocCDF16, AllocCommand>
    where <AllocU8 as Allocator<u8>>::AllocatedMemory: core::marker::Send,
          <AllocCDF16 as Allocator<interface::DefaultCDF16>>::AllocatedMemory: core::marker::Send,
          <AllocCommand as Allocator<StaticCommand>>::AllocatedMemory: core::marker::Send,
{

    pub fn new(header: &mut HeaderParser<AllocU8, AllocCDF16, AllocCommand>, mut window_size: usize) -> Self {
        if window_size < 10 {
            window_size = 10;
        }
        if window_size > 24 {
            window_size = 24;
        }
        let mut m8:AllocU8;
        let mcdf16:AllocCDF16;
        let mut mc: AllocCommand;
        let raw_header:[u8; interface::HEADER_LENGTH];
        let skip_crc:bool;
        m8 = header.m8.take().unwrap();
        raw_header = header.header;
        skip_crc = header.skip_crc;
        mcdf16 = header.mcdf16.take().unwrap();
        mc = header.mcommand.take().unwrap();
        //update this if you change the SelectedArithmeticDecoder macro
        let cmd_decoder = DefaultDecoder::new(&mut m8);
        let lit_decoder = DefaultDecoder::new(&mut m8);
        let linear_input_bytes = ThreadToMainDemuxer::<AllocU8,BufferedMultiWorker<AllocU8, AllocCommand>>::new(
            BufferedMultiWorker::<AllocU8, AllocCommand>::new(&mut mc));
        let mut codec = codec::DivansCodec::<DefaultDecoder,
                                             DecoderSpecialization,
                                             ThreadToMainDemuxer<AllocU8, BufferedMultiWorker<AllocU8, AllocCommand>>,
                                             DevNull<AllocU8>,
                                             interface::DefaultCDF16,
                                             AllocU8,
                                             AllocCDF16>::new(m8,
                                                              mcdf16,
                                                              cmd_decoder,
                                                              lit_decoder,
                                                              DecoderSpecialization::new(),
                                                              linear_input_bytes,
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
        let main_thread_codec = codec.fork(&mut mc);
        assert_eq!(*codec.get_crc(), main_thread_codec.crc);
        let multi_worker = (codec.demuxer().worker).worker.clone();
        let thread_codec = Arc::new(Mutex::new(Some(codec)));
        let worker_codec = thread_codec.clone();
        thread::spawn(move || {
            let mut guard = thread_codec.lock().unwrap();
            if let Some(ref mut process_codec) = *guard {
                let mut unused_out = 0usize;
                let mut unused_in = 0usize;
                let mut unused = 0usize;
                loop {
                    match process_codec.encode_or_decode(
                        &[],
                        &mut unused_in,
                        &mut [],
                        &mut unused_out,
                        &codec::EmptyCommandArray::default(),
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
        ParallelDivansProcess::<DefaultDecoder, AllocU8, AllocCDF16, AllocCommand> {
            mcommand:mc,
            codec:worker_codec,
            literal_decoder:Some(main_thread_codec),
            bytes_encoded:0,
            worker: multi_worker,
        }
    }
    pub fn free_ref(&mut self) {
        if let Some(ref mut codec) = *self.codec.lock().unwrap() {
            let lit_decoder = core::mem::replace(&mut self.literal_decoder, None);
            if let Some(ld) = lit_decoder {
                codec.join(ld, &mut self.mcommand);
            }
            codec.cross_command_state.demuxer.worker.free(codec.cross_command_state.thread_ctx.m8().as_mut().unwrap(), &mut self.mcommand);

            codec.free_ref();
        }
    }
    pub fn free(mut self) -> (AllocU8, AllocCDF16, AllocCommand) {
        use codec::NUM_ARITHMETIC_CODERS;
        if let Some(mut codec) = core::mem::replace(&mut *self.codec.lock().unwrap(), None) {
            let lit_decoder = core::mem::replace(&mut self.literal_decoder, None);
            if let Some(ld) = lit_decoder {
                codec.join(ld, &mut self.mcommand);
            }
            for index in 0..NUM_ARITHMETIC_CODERS {
                codec.get_coder(index as u8).debug_print(self.bytes_encoded);
            }
            codec.cross_command_state.demuxer.worker.free(codec.cross_command_state.thread_ctx.m8().as_mut().unwrap(), &mut self.mcommand);
            let (m8,mcdf) = codec.free();
            (m8, mcdf, self.mcommand)
        } else {
            panic!("Trying to free unjoined decoder"); //FIXME: this does not seem ergonomic
        }
    }
    pub fn decode(&mut self,
              input:&[u8],
              input_offset:&mut usize,
              output:&mut [u8],
              output_offset: &mut usize) -> DivansResult {
        let old_output_offset = *output_offset;
        if let Some(literal_decoder) =  self.literal_decoder.as_mut() {
            loop {
                match literal_decoder.decode_process_input(&mut self.worker,
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
                &mut self.worker,
                output,
                output_offset);
            self.bytes_encoded += *output_offset - old_output_offset;
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
    }
}

