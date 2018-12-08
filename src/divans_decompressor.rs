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


use ::interface::{DivansResult, DivansOpResult, DivansInputResult, ErrMsg};
use ::ArithmeticEncoderOrDecoder;
use ::alloc::{Allocator};
pub use threading::StaticCommand;

#[cfg(feature="std")]
use parallel_decompressor::{ParallelDivansProcess};
#[cfg(not(feature="std"))]
use stub_parallel_decompressor::{ParallelDivansProcess};

pub struct HeaderParser<AllocU8:Allocator<u8>,
                        AllocCDF16:Allocator<interface::DefaultCDF16>,
                        AllocCommand:Allocator<StaticCommand>> {
    pub header:[u8;interface::HEADER_LENGTH],
    pub read_offset: usize,
    pub m8: Option<AllocU8>,
    pub mcdf16: Option<AllocCDF16>,
    pub mcommand: Option<AllocCommand>,
    pub skip_crc: bool,
    pub multithread: bool,
}

impl<AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocCommand:Allocator<StaticCommand>>HeaderParser<AllocU8, AllocCDF16, AllocCommand> {
    pub fn parse_header(&mut self)->Result<usize, DivansOpResult>{
        if self.header[0] != interface::MAGIC_NUMBER[0] ||
            self.header[1] != interface::MAGIC_NUMBER[1] {
                return Err(DivansOpResult::Failure(ErrMsg::MagicNumberWrongA(self.header[0], self.header[1])));
        }
        if self.header[2] != interface::MAGIC_NUMBER[2] ||
            self.header[3] != interface::MAGIC_NUMBER[3] {
                return Err(DivansOpResult::Failure(ErrMsg::MagicNumberWrongB(self.header[2], self.header[3])));
        }
        let window_size = self.header[5] as usize;
        if window_size < 10 || window_size >= 25 {
            return Err(DivansOpResult::Failure(ErrMsg::BadWindowSize(window_size as u8)));
        }
        Ok(window_size)
    }
    pub fn decode(&mut self,
                  input:&[u8],
                  input_offset:&mut usize) -> (usize, bool, DivansInputResult) {
        let header_parser = self;
        let window_size: usize;
        let is_multi: bool;
        let remaining = input.len() - *input_offset;
        let header_left = header_parser.header.len() - header_parser.read_offset;
        if remaining >= header_left {
            header_parser.header[header_parser.read_offset..].clone_from_slice(
                input.split_at(*input_offset).1.split_at(header_left).0);
            *input_offset += header_left;
            match header_parser.parse_header() {
                Ok(wsize) => {
                    window_size = wsize;
                    is_multi = header_parser.multithread;
                },
                Err(result) => return (0, false, DivansInputResult::from(result)),
            }
        } else {
            header_parser.header[(header_parser.read_offset)..
                                 (header_parser.read_offset+remaining)].clone_from_slice(
                input.split_at(*input_offset).1);
            *input_offset += remaining;
            header_parser.read_offset += remaining;
            return (0, false, DivansInputResult::NeedsMoreInput);
        }
        (window_size, is_multi, DivansInputResult::Success)
    }
}

pub struct DivansProcess<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                         Parser:codec::StructureSeekerU8<AllocU8>,
                         AllocU8:Allocator<u8>,
                         AllocCDF16:Allocator<interface::DefaultCDF16>,
                         AllocCommand:Allocator<StaticCommand>> {
    codec: Option<codec::DivansCodec<DefaultDecoder,
                                     DecoderSpecialization,
                                     ThreadToMainDemuxer<AllocU8, SerialWorker<AllocU8, AllocCommand>>,
                                     DevNull<AllocU8>,
                                     interface::DefaultCDF16,
                                     AllocU8,
                                     AllocCDF16,
                                     Parser>>,
    literal_decoder: Option<DivansDecoderCodec<interface::DefaultCDF16,
                                               Parser,
                                               AllocU8,
                                               AllocCDF16,
                                               AllocCommand,
                                               DefaultDecoder,
                                               Mux<AllocU8>>>,
    bytes_encoded: usize,
    mcommand: AllocCommand,
}



impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
     Parser: codec::StructureSeekerU8<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocCommand:Allocator<StaticCommand>> DivansProcess<DefaultDecoder, Parser, AllocU8, AllocCDF16, AllocCommand> {
    fn decode(&mut self,
              input:&[u8],
              input_offset:&mut usize,
              output:&mut [u8],
              output_offset: &mut usize) -> DivansResult {
        let process = self;
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
            match process.codec.as_mut().unwrap().encode_or_decode(
                &[],
                &mut unused_in,
                &mut [],
                &mut unused_out,
                &codec::EmptyCommandArray::default(),
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
    }
    pub fn free(mut self) -> (AllocU8, AllocCDF16, AllocCommand) {
        use codec::NUM_ARITHMETIC_CODERS;
        if let Some(mut codec) = core::mem::replace(&mut self.codec, None) {
            let lit_decoder = core::mem::replace(&mut self.literal_decoder, None);
            if let Some(ld) = lit_decoder {
                codec.join(ld, &mut self.mcommand);
            }
            for index in 0..NUM_ARITHMETIC_CODERS {
                codec.get_coder(index as u8).debug_print(self.bytes_encoded);
            }
            codec.cross_command_state.demuxer.worker.free(codec.cross_command_state.thread_ctx.m8().as_mut().unwrap(), &mut self.mcommand);
            let (m8, mcdf) = codec.free();
            (m8, mcdf, self.mcommand)
        } else {
            panic!("Trying to free unjoined decoder"); //FIXME: this does not seem ergonomic
        }
    }
    pub fn free_ref(&mut self) {
        if let Some(ref mut codec) = self.codec {
            let lit_decoder = core::mem::replace(&mut self.literal_decoder, None);
            if let Some(ld) = lit_decoder {
                codec.join(ld, &mut self.mcommand);
            }
            codec.cross_command_state.demuxer.worker.free(codec.cross_command_state.thread_ctx.m8().as_mut().unwrap(), &mut self.mcommand);
            codec.free_ref();
        }
    }
}

pub enum DivansDecompressor<
        DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
Parser:codec::StructureSeekerU8<AllocU8>,
AllocU8:Allocator<u8>,
AllocCDF16:Allocator<interface::DefaultCDF16>,
AllocCommand:Allocator<StaticCommand>> {
    Header(HeaderParser<AllocU8, AllocCDF16, AllocCommand>),
    Decode(DivansProcess<DefaultDecoder,
           Parser,
           AllocU8,
           AllocCDF16,
           AllocCommand>),
    MultiDecode(ParallelDivansProcess<DefaultDecoder,
                Parser,
                AllocU8,
                AllocCDF16,
                AllocCommand>),
}

impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
     Parser: codec::StructureSeekerU8<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocCommand:Allocator<StaticCommand>>
    DivansDecompressor<DefaultDecoder, Parser, AllocU8, AllocCDF16, AllocCommand> {

    fn finish_parsing_header_serial(&mut self, window_size: usize) -> DivansResult {
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
        let mut mcommand:AllocCommand;
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
        match *self {
            DivansDecompressor::Header(ref mut header) => {
                mcommand = match core::mem::replace(&mut header.mcommand, None) {
                    None => return DivansResult::Failure(ErrMsg::MissingAllocator(32)),
                    Some(m) => m,
                }
            },
            _ => return DivansResult::Failure(ErrMsg::WrongInternalDecoderState),
        }
        //update this if you change the SelectedArithmeticDecoder macro
        let cmd_decoder = DefaultDecoder::new(&mut m8);
        let lit_decoder = DefaultDecoder::new(&mut m8);
        let linear_input_bytes = ThreadToMainDemuxer::<AllocU8,SerialWorker<AllocU8, AllocCommand>>::new(
            SerialWorker::<AllocU8, AllocCommand>::new(&mut mcommand));
        let mut codec = codec::DivansCodec::<DefaultDecoder,
                                             DecoderSpecialization,
                                             ThreadToMainDemuxer<AllocU8, SerialWorker<AllocU8, AllocCommand>>,
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
        let main_thread_codec = codec.fork(&mut mcommand);
        assert_eq!(*codec.get_crc(), main_thread_codec.crc);
        core::mem::replace(self,
                           DivansDecompressor::Decode(
                               DivansProcess::<DefaultDecoder, Parser, AllocU8, AllocCDF16, AllocCommand> {
                                   codec:Some(codec),
                                   literal_decoder:Some(main_thread_codec),
                                   bytes_encoded:0,
                                   mcommand:mcommand,
                               }));
        DivansResult::Success
    }
}


macro_rules! free_body {
    () => {
    pub fn free_ref(&mut self) {
        match self {
            DivansDecompressor::Header(_parser) => {},
            DivansDecompressor::MultiDecode(ref mut process) => {
                process.free_ref()
            },
            DivansDecompressor::Decode(ref mut process) => {
                process.free_ref()
            }
        }
    }
    pub fn free(self) -> (AllocU8, AllocCDF16, AllocCommand) {
        match self {
            DivansDecompressor::Header(parser) => {
                (parser.m8.unwrap(),
                 parser.mcdf16.unwrap(),
                 parser.mcommand.unwrap(),
                )
            },
            DivansDecompressor::MultiDecode(process) => {
                process.free()
            },
            DivansDecompressor::Decode(process) => {
                process.free()
            }
        }
    }
    }
}
#[cfg(feature="std")]
impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
     Parser: codec::StructureSeekerU8<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocCommand:Allocator<StaticCommand>>
    DivansDecompressor<DefaultDecoder, Parser, AllocU8, AllocCDF16, AllocCommand>
    where
        DefaultDecoder: Send + 'static,  // fixme: only demand send if not no-stdlib
        AllocCommand : Send + 'static,
        AllocCDF16 : Send + 'static,
        AllocU8 : Send + 'static,
        AllocCommand::AllocatedMemory : Send + 'static,
        AllocCDF16::AllocatedMemory : Send + 'static,
        AllocU8::AllocatedMemory : Send + 'static,
        {
    free_body!();
}

#[cfg(not(feature="std"))]
impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocCommand:Allocator<StaticCommand>>
    DivansDecompressor<DefaultDecoder, AllocU8, AllocCDF16, AllocCommand> {
        free_body!();
}


macro_rules! decode_body {
    () => {
    fn decode(&mut self,
              input:&[u8],
              input_offset:&mut usize,
              output:&mut [u8],
              output_offset: &mut usize) -> DivansResult {
        let window_size: usize;
        let is_multi: bool;
        match *self  {
            DivansDecompressor::Header(ref mut header_parser) => {
                let (ws, mul, ret) = header_parser.decode(input, input_offset);
                if let DivansInputResult::Success = ret {
                    window_size = ws;
                    is_multi = mul;
                } else {
                    return DivansResult::from(ret);
                }
            },
            DivansDecompressor::MultiDecode(ref mut process) => {
                return process.decode(input, input_offset, output, output_offset);
            },
            DivansDecompressor::Decode(ref mut process) => {
                return process.decode(input, input_offset, output, output_offset);
            },
        }
        if is_multi {
            let par_proc;
            {
                if let DivansDecompressor::Header(ref mut header) = *self {
                    par_proc = ParallelDivansProcess::<DefaultDecoder, AllocU8, AllocCDF16, AllocCommand>::new(header, window_size);
                } else {
                    return DivansResult::Failure(ErrMsg::WrongInternalDecoderState);
                }
            }
            *self = DivansDecompressor::MultiDecode(par_proc);
        } else {
            self.finish_parsing_header_serial(window_size);
        }
        if *input_offset < input.len() {
            return self.decode(input, input_offset, output, output_offset);
        }
        DivansResult::NeedsMoreInput
    }
        
    }
}

#[cfg(feature="std")]
impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
     Parser: codec::StructureSeekerU8<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocCommand:Allocator<StaticCommand>,
     > Decompressor for DivansDecompressor<DefaultDecoder,
                                           Parser,
                                           AllocU8,
                                           AllocCDF16,
                                           AllocCommand>
    where
        DefaultDecoder: Send + 'static,  // fixme: only demand send if not no-stdlib
        AllocCommand : Send + 'static,
        AllocCDF16 : Send + 'static,
        AllocU8 : Send + 'static,
        AllocCommand::AllocatedMemory : Send + 'static,
        AllocCDF16::AllocatedMemory : Send + 'static,
        AllocU8::AllocatedMemory : Send + 'static,
{
    decode_body!();
}

#[cfg(not(feature="std"))]
impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
     Parser: codec::StructureSeekerU8<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocCommand:Allocator<StaticCommand>> Decompressor for DivansDecompressor<DefaultDecoder,
                                                                                Parser,
                                                                                AllocU8,
                                                                                AllocCDF16,
                                                                                AllocCommand> {
    decode_body!();
}

pub trait DivansDecompressorFactory<
     AllocU8:Allocator<u8>,
    AllocCDF16:Allocator<interface::DefaultCDF16>,
    AllocCommand:Allocator<StaticCommand>,
    Parser: codec::StructureSeekerU8<AllocU8>,> {
    type DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>;
    fn new(m8: AllocU8,
           mcdf16:AllocCDF16,
           mc: AllocCommand,
           skip_crc:bool,
           multithread:bool) -> DivansDecompressor<Self::DefaultDecoder, Parser, AllocU8, AllocCDF16, AllocCommand> {
        DivansDecompressor::Header(HeaderParser{header:[0u8;interface::HEADER_LENGTH], read_offset:0,
                                                m8:Some(m8), mcdf16:Some(mcdf16), mcommand:Some(mc),
                                                skip_crc:skip_crc,
                                                multithread:multithread,
        })
    }
}

#[derive(Default)]
pub struct DivansDecompressorFactoryStruct
    <AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocCommand:Allocator<StaticCommand>,
     Parser: codec::StructureSeekerU8<AllocU8>,
     > {
    p1: PhantomData<AllocU8>,
    p2: PhantomData<AllocCDF16>,
    p3: PhantomData<AllocCommand>,
}

impl<AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocCommand:Allocator<StaticCommand>,
     Parser: codec::StructureSeekerU8<AllocU8>,> DivansDecompressorFactory<AllocU8,
                                                                           AllocCDF16,
                                                                           AllocCommand,
                                                                           Parser>
    for DivansDecompressorFactoryStruct<AllocU8, AllocCDF16, AllocCommand, Parser> {
     type DefaultDecoder = DefaultDecoderType!();
}
