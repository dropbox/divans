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

pub mod interface;
pub mod slice_util;
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
mod billing;
mod ans;
pub mod constants;
pub use brotli_decompressor::{BrotliResult};
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use interface::{BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd};
pub use cmd_to_raw::DivansRecodeState;
pub use codec::CMD_BUFFER_SIZE;
pub use divans_to_raw::DecoderSpecialization;
pub use cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec};

use core::marker::PhantomData;

const HEADER_LENGTH: usize = 16;
const MAGIC_NUMBER:[u8;4] = [0xff, 0xe5,0x8c, 0x9f];

pub use probability::Speed;
#[cfg(feature="blend")]
#[cfg(not(feature="debug_entropy"))]
pub type DefaultCDF16 = probability::BlendCDF16;
#[cfg(not(feature="blend"))]
#[cfg(not(feature="debug_entropy"))]
pub type DefaultCDF16 = probability::FrequentistCDF16;
#[cfg(feature="blend")]
#[cfg(feature="debug_entropy")]
pub type DefaultCDF16 = probability::DebugWrapperCDF16<probability::BlendCDF16>;
#[cfg(not(feature="blend"))]
#[cfg(feature="debug_entropy")]
pub type DefaultCDF16 = probability::DebugWrapperCDF16<probability::FrequentistCDF16>;

pub use probability::CDF2;

#[cfg(not(feature="billing"))]
macro_rules! DefaultEncoderType(
    () => {ans::EntropyEncoderANS<AllocU8>}
);

#[cfg(not(feature="billing"))]
macro_rules! DefaultDecoderType(
    () => {ans::EntropyDecoderANS<AllocU8>}
);


#[cfg(feature="billing")]
macro_rules! DefaultEncoderType(
    () => { billing::BillingArithmeticCoder<AllocU8, ans::EntropyEncoderANS<AllocU8>> }
);

#[cfg(feature="billing")]
macro_rules! DefaultDecoderType(
    () => { billing::BillingArithmeticCoder<AllocU8, ans::EntropyDecoderANS<AllocU8>> }
);

const COMPRESSOR_CMD_BUFFER_SIZE : usize = 16;
pub struct DivansCompressor<DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                            AllocU8:Allocator<u8>,
                            AllocU32:Allocator<u32>,
                            AllocCDF2:Allocator<probability::CDF2>,
                            AllocCDF16:Allocator<DefaultCDF16>> {
    m32: AllocU32,
    codec: DivansCodec<DefaultEncoder, EncoderSpecialization, DefaultCDF16, AllocU8, AllocCDF2, AllocCDF16>,
    header_progress: usize,
    window_size: u8,
    cmd_assembler: raw_to_cmd::RawToCmdState<AllocU8::AllocatedMemory, AllocU32>,
    freeze_dried_cmd_array: [Command<slice_util::SliceReference<'static,u8>>; COMPRESSOR_CMD_BUFFER_SIZE],
    freeze_dried_cmd_start: usize,
    freeze_dried_cmd_end: usize,
}

pub trait DivansCompressorFactory<
     AllocU8:Allocator<u8>, 
     AllocU32:Allocator<u32>, 
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> {
     type DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>;
    fn new(mut m8: AllocU8, mut m32: AllocU32, mcdf2:AllocCDF2, mcdf16:AllocCDF16,mut window_size: usize,
           literal_adaptation_rate: Option<probability::Speed>) -> 
        DivansCompressor<Self::DefaultEncoder, AllocU8, AllocU32, AllocCDF2, AllocCDF16> {
        if window_size < 10 {
            window_size = 10;
        }
        if window_size > 24 {
            window_size = 24;
        }
        let ring_buffer = m8.alloc_cell(1<<window_size);
        let enc = Self::DefaultEncoder::new(&mut m8);
        let assembler = raw_to_cmd::RawToCmdState::new(&mut m32, ring_buffer);
          DivansCompressor::<Self::DefaultEncoder, AllocU8, AllocU32, AllocCDF2, AllocCDF16> {
            m32 :m32,
            codec:DivansCodec::<Self::DefaultEncoder, EncoderSpecialization, DefaultCDF16, AllocU8, AllocCDF2, AllocCDF16>::new(
                m8,
                mcdf2,
                mcdf16,
                enc,
                EncoderSpecialization::new(),
                window_size,
                literal_adaptation_rate,
            ),
              freeze_dried_cmd_array:[interface::Command::<slice_util::SliceReference<'static, u8>>::default(); COMPRESSOR_CMD_BUFFER_SIZE],
            freeze_dried_cmd_start:0,
            freeze_dried_cmd_end:0,
            cmd_assembler:assembler,
            header_progress: 0,
            window_size: window_size as u8,
        }
    }
}

pub struct DivansCompressorFactoryStruct
    <AllocU8:Allocator<u8>, 
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> {
    p1: PhantomData<AllocU8>,
    p2: PhantomData<AllocCDF2>,
    p3: PhantomData<AllocCDF16>,
}

impl<AllocU8:Allocator<u8>,
     AllocU32:Allocator<u32>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> DivansCompressorFactory<AllocU8, AllocU32, AllocCDF2, AllocCDF16>
    for DivansCompressorFactoryStruct<AllocU8, AllocCDF2, AllocCDF16> {
     type DefaultEncoder = DefaultEncoderType!();
}

fn make_header(window_size: u8) -> [u8; HEADER_LENGTH] {
    let mut retval = [0u8; HEADER_LENGTH];
    retval[0..MAGIC_NUMBER.len()].clone_from_slice(&MAGIC_NUMBER[..]);
    retval[5] = window_size;
    retval
}
fn thaw_commands<'a>(input: &[Command<slice_util::SliceReference<'static, u8>>], ring_buffer: &'a[u8], start_index:  usize, end_index: usize) -> [Command<slice_util::SliceReference<'a, u8>>; COMPRESSOR_CMD_BUFFER_SIZE] {
   let mut ret : [Command<slice_util::SliceReference<'a, u8>>; COMPRESSOR_CMD_BUFFER_SIZE] = [Command::<slice_util::SliceReference<u8>>::default(); COMPRESSOR_CMD_BUFFER_SIZE];
   for (thawed, frozen) in ret[start_index..end_index].iter_mut().zip(input[start_index..end_index].iter()) {
      *thawed = *frozen;
   }
   for item in ret[start_index..end_index].iter_mut() {
       match item {
       &mut Command::Literal(ref mut lit) => {
           lit.data = lit.data.thaw(ring_buffer);
       },
       &mut Command::PredictionMode(ref mut pm) => {
           pm.literal_context_map = pm.literal_context_map.thaw(ring_buffer);
           pm.distance_context_map = pm.distance_context_map.thaw(ring_buffer);
       },
       _ => {},       
       }
//       item.apply_array(|array_item:&mut slice_util::SliceReference<'a, u8>| *array_item = array_item.thaw(ring_buffer));
   }
   ret
}
impl<DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>, AllocU8:Allocator<u8>, AllocU32:Allocator<u32>, AllocCDF2:Allocator<probability::CDF2>, AllocCDF16:Allocator<DefaultCDF16>> 
    DivansCompressor<DefaultEncoder, AllocU8, AllocU32, AllocCDF2, AllocCDF16> {
    fn flush_freeze_dried_cmds(&mut self, output: &mut [u8], output_offset: &mut usize) -> BrotliResult {
        if self.freeze_dried_cmd_start != self.freeze_dried_cmd_end { // we have some freeze dried items
            let mut thawed_buffer = thaw_commands(&self.freeze_dried_cmd_array[..], self.cmd_assembler.ring_buffer.slice(),
                                                  self.freeze_dried_cmd_start, self.freeze_dried_cmd_end);
            let mut unused: usize = 0;
            match self.codec.encode_or_decode(&[],
                                    &mut unused,
                                    output,
                                    output_offset,
                                    thawed_buffer.split_at(self.freeze_dried_cmd_end).0,
                                    &mut self.freeze_dried_cmd_start) {
               BrotliResult::ResultFailure => return BrotliResult::ResultFailure,
               BrotliResult::NeedsMoreInput | BrotliResult::ResultSuccess => {},
               BrotliResult::NeedsMoreOutput => return BrotliResult::NeedsMoreOutput,
            }
        }
        BrotliResult::ResultSuccess
    }
        fn freeze_dry<'a>(freeze_dried_cmd_array: &mut[Command<slice_util::SliceReference<'static, u8>>;COMPRESSOR_CMD_BUFFER_SIZE],
                          freeze_dried_cmd_start: &mut usize,
                          freeze_dried_cmd_end: &mut usize,
                          input:&[Command<slice_util::SliceReference<'a, u8>>]) {
        assert!(input.len() <= freeze_dried_cmd_array.len());
        *freeze_dried_cmd_start = 0;
        *freeze_dried_cmd_end = freeze_dried_cmd_array.len();
        for (frozen, leftover) in freeze_dried_cmd_array.split_at_mut(input.len()).0.iter_mut().zip(input.iter()) {
            *frozen = match leftover {
                &Command::Literal(ref lit) => {
                    Command::Literal(LiteralCommand::<slice_util::SliceReference<'static, u8>> {
                        data: lit.data.freeze_dry(),
                    })
                },
                &Command::PredictionMode(ref pm) => {
                    Command::PredictionMode(PredictionModeContextMap::<slice_util::SliceReference<'static, u8>> {
                        literal_prediction_mode: pm.literal_prediction_mode.clone(),
                        literal_context_map: pm.literal_context_map.freeze_dry(),
                        distance_context_map: pm.literal_context_map.freeze_dry(),
                    })
                },
                &Command::Copy(ref c) => {
                    Command::Copy(c.clone())
                }
                &Command::Dict(ref d) => {
                    Command::Dict(d.clone())
                }
                &Command::BlockSwitchLiteral(ref l) => {
                    Command::BlockSwitchLiteral(l.clone())
                }
                &Command::BlockSwitchCommand(ref c) => {
                    Command::BlockSwitchCommand(c.clone())
                }
                &Command::BlockSwitchDistance(ref d) => {
                    Command::BlockSwitchDistance(d.clone())
                }
            };
        }
    }
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
    pub fn get_m8(&mut self) -> &mut AllocU8 {
       self.codec.get_m8()
    }
}

impl<DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocU32:Allocator<u32>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> Compressor for DivansCompressor<DefaultEncoder, AllocU8, AllocU32, AllocCDF2, AllocCDF16>   {
    fn encode(&mut self,
              input: &[u8],
              input_offset: &mut usize,
              output: &mut [u8],
              output_offset: &mut usize) -> BrotliResult {
        if self.header_progress != HEADER_LENGTH {
            match self. write_header(output, output_offset) {
                BrotliResult::ResultSuccess => {},
                res => return res,
            }
        }
        match self.flush_freeze_dried_cmds(output, output_offset) {
            BrotliResult::NeedsMoreInput | BrotliResult::ResultSuccess => {},
            BrotliResult::ResultFailure => return BrotliResult::ResultFailure,
            BrotliResult::NeedsMoreOutput => return BrotliResult::NeedsMoreOutput,
        }
        loop {
            let mut temp_bs: [interface::Command<slice_util::SliceReference<u8>>;COMPRESSOR_CMD_BUFFER_SIZE] =
                [interface::Command::<slice_util::SliceReference<u8>>::default();COMPRESSOR_CMD_BUFFER_SIZE];
            let mut temp_cmd_offset = 0;
            let command_decode_ret = self.cmd_assembler.stream(&input, input_offset,
                                                               &mut temp_bs[..], &mut temp_cmd_offset);
            match command_decode_ret {
                BrotliResult::NeedsMoreInput => {
                    if temp_cmd_offset == 0 {
                        // nothing to freeze dry, return
                        return BrotliResult::NeedsMoreInput;
                    }
                },
                BrotliResult::ResultFailure | BrotliResult::ResultSuccess => {
                    return BrotliResult::ResultFailure; // we are never done
                },
                BrotliResult::NeedsMoreOutput => {},
            }
            let mut out_cmd_offset = 0;
            let mut zero: usize = 0;
            let codec_ret = self.codec.encode_or_decode(&[],
                                                        &mut zero,
                                                        output,
                                                        output_offset,
                                                        temp_bs.split_at(temp_cmd_offset).0,
                                                        &mut out_cmd_offset);
            match codec_ret {
                BrotliResult::NeedsMoreInput | BrotliResult::ResultSuccess => {
                    assert_eq!(temp_cmd_offset, out_cmd_offset); // must have consumed all commands
                    match command_decode_ret {
                        BrotliResult::NeedsMoreInput => return BrotliResult::NeedsMoreInput, // we've exhausted all commands and all input
                        _ => {},
                    }
                },
                BrotliResult::NeedsMoreOutput | BrotliResult::ResultFailure => {
                    Self::freeze_dry(
                        &mut self.freeze_dried_cmd_array,
                        &mut self.freeze_dried_cmd_start,
                        &mut self.freeze_dried_cmd_end,
                        &temp_bs[out_cmd_offset..temp_cmd_offset]);
                    return codec_ret;
                }
            }
        }
    }
    fn encode_commands<SliceType:SliceWrapper<u8>+Default>(&mut self,
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
        match self.flush_freeze_dried_cmds(output, output_offset) {
               BrotliResult::ResultFailure => return BrotliResult::ResultFailure, 
               BrotliResult::NeedsMoreOutput => return BrotliResult::NeedsMoreOutput,
               BrotliResult::NeedsMoreInput | BrotliResult::ResultSuccess => {},
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

pub enum DivansDecompressor<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                            AllocU8:Allocator<u8>,
                            AllocCDF2:Allocator<probability::CDF2>,
                            AllocCDF16:Allocator<DefaultCDF16>> {
    Header(HeaderParser<AllocU8, AllocCDF2, AllocCDF16>),
    Decode(DivansCodec<DefaultDecoder, DecoderSpecialization, DefaultCDF16, AllocU8, AllocCDF2, AllocCDF16>, usize),
}

pub trait DivansDecompressorFactory<
     AllocU8:Allocator<u8>, 
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> {
     type DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>;
    fn new(m8: AllocU8, mcdf2:AllocCDF2, mcdf16:AllocCDF16) -> DivansDecompressor<Self::DefaultDecoder, AllocU8, AllocCDF2, AllocCDF16> {
        DivansDecompressor::Header(HeaderParser{header:[0u8;HEADER_LENGTH], read_offset:0, m8:Some(m8), mcdf2:Some(mcdf2), mcdf16:Some(mcdf16)})
    }
}

impl<DefaultDecoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8> + interface::BillingCapability,
                        AllocU8:Allocator<u8>,
                        AllocCDF2:Allocator<probability::CDF2>,
                        AllocCDF16:Allocator<DefaultCDF16>>  
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
        let decoder = DefaultDecoder::new(&mut m8);
        core::mem::replace(self,
                           DivansDecompressor::Decode(DivansCodec::<DefaultDecoder,
                                                                    DecoderSpecialization,
                                                                    DefaultCDF16,
                                                                    AllocU8,
                                                                    AllocCDF2,
                                                                    AllocCDF16>::new(m8,
                                                                                     mcdf2,
                                                                                     mcdf16,
                                                                                     decoder,
                                                                                     DecoderSpecialization::new(),
                                                                                     window_size,
                                                                                     None), 0));
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
     AllocCDF16:Allocator<DefaultCDF16>> Decompressor for DivansDecompressor<DefaultDecoder, AllocU8, AllocCDF2, AllocCDF16> {
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

pub struct DivansDecompressorFactoryStruct
    <AllocU8:Allocator<u8>, 
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> {
    p1: PhantomData<AllocU8>,
    p2: PhantomData<AllocCDF2>,
    p3: PhantomData<AllocCDF16>,
}

impl<AllocU8:Allocator<u8>, 
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<DefaultCDF16>> DivansDecompressorFactory<AllocU8, AllocCDF2, AllocCDF16>
    for DivansDecompressorFactoryStruct<AllocU8, AllocCDF2, AllocCDF16> {
     type DefaultDecoder = DefaultDecoderType!();
}


