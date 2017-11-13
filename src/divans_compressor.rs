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

// this compressor generates its own IR through the raw_to_cmd command assembler
// then it generates a valid divans bitstream using the ANS encoder

use core::marker::PhantomData;
use super::probability;
use super::brotli;
use super::raw_to_cmd;
use super::slice_util;
pub use super::alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use super::interface::{BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType};

pub use super::cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec};
use super::interface;
use super::brotli::BrotliResult;
const COMPRESSOR_CMD_BUFFER_SIZE : usize = 16;
pub struct DivansCompressor<DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                            AllocU8:Allocator<u8>,
                            AllocU32:Allocator<u32>,
                            AllocCDF2:Allocator<probability::CDF2>,
                            AllocCDF16:Allocator<interface::DefaultCDF16>> {
    m32: AllocU32,
    codec: DivansCodec<DefaultEncoder, EncoderSpecialization, interface::DefaultCDF16, AllocU8, AllocCDF2, AllocCDF16>,
    header_progress: usize,
    window_size: u8,
    cmd_assembler: raw_to_cmd::RawToCmdState<AllocU8::AllocatedMemory, AllocU32>,
    freeze_dried_cmd_array: [Command<slice_util::SliceReference<'static,u8>>; COMPRESSOR_CMD_BUFFER_SIZE],
    freeze_dried_cmd_start: usize,
    freeze_dried_cmd_end: usize,
}


pub struct DivansCompressorFactoryStruct
    <AllocU8:Allocator<u8>, 
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> {
    p1: PhantomData<AllocU8>,
    p2: PhantomData<AllocCDF2>,
    p3: PhantomData<AllocCDF16>,
}

impl<AllocU8:Allocator<u8>,
     AllocU32:Allocator<u32>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> interface::DivansCompressorFactory<AllocU8,
                                                                                       AllocU32,
                                                                                       AllocCDF2,
                                                                                       AllocCDF16>
    for DivansCompressorFactoryStruct<AllocU8, AllocCDF2, AllocCDF16> {
     type DefaultEncoder = DefaultEncoderType!();
     type ConstructedCompressor = DivansCompressor<Self::DefaultEncoder, AllocU8, AllocU32, AllocCDF2, AllocCDF16>;
     type AdditionalArgs = ();
     fn new(mut m8: AllocU8, mut m32: AllocU32, mcdf2:AllocCDF2, mcdf16:AllocCDF16,mut window_size: usize,
           literal_adaptation_rate: Option<probability::Speed>,
           _additional_args: ()) -> DivansCompressor<Self::DefaultEncoder, AllocU8, AllocU32, AllocCDF2, AllocCDF16> {
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
            codec:DivansCodec::<Self::DefaultEncoder, EncoderSpecialization, interface::DefaultCDF16, AllocU8, AllocCDF2, AllocCDF16>::new(
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

pub fn make_header(window_size: u8) -> [u8; interface::HEADER_LENGTH] {
    let mut retval = [0u8; interface::HEADER_LENGTH];
    retval[0..interface::MAGIC_NUMBER.len()].clone_from_slice(&interface::MAGIC_NUMBER[..]);
    retval[5] = window_size;
    retval
}
fn thaw_commands<'a>(input: &[Command<slice_util::SliceReference<'static, u8>>], ring_buffer: &'a[u8], start_index:  usize, end_index: usize) -> [Command<slice_util::SliceReference<'a, u8>>; COMPRESSOR_CMD_BUFFER_SIZE] {
   let mut ret : [Command<slice_util::SliceReference<'a, u8>>; COMPRESSOR_CMD_BUFFER_SIZE] = [Command::<slice_util::SliceReference<u8>>::default(); COMPRESSOR_CMD_BUFFER_SIZE];
   for (thawed, frozen) in ret[start_index..end_index].iter_mut().zip(input[start_index..end_index].iter()) {
      *thawed = *frozen;
   }
   for item in ret[start_index..end_index].iter_mut() {
       match *item {
       Command::Literal(ref mut lit) => {
           lit.data = lit.data.thaw(ring_buffer);
           assert_eq!(lit.prob.slice().len(), 0);
       },
       Command::PredictionMode(ref mut pm) => {
           pm.literal_context_map = pm.literal_context_map.thaw(ring_buffer);
           pm.distance_context_map = pm.distance_context_map.thaw(ring_buffer);
       },
       _ => {},       
       }
//       item.apply_array(|array_item:&mut slice_util::SliceReference<'a, u8>| *array_item = array_item.thaw(ring_buffer));
   }
   ret
}
#[cfg(not(feature="external-literal-probability"))]
fn freeze_dry<'a>(_item: &FeatureFlagSliceType<slice_util::SliceReference<'a, u8>>) -> FeatureFlagSliceType<slice_util::SliceReference<'static, u8>> {
    FeatureFlagSliceType::<slice_util::SliceReference<'static, u8>>::default()
}

#[cfg(feature="external-literal-probability")]
fn freeze_dry<'a>(item: &FeatureFlagSliceType<slice_util::SliceReference<'a, u8>>) -> FeatureFlagSliceType<slice_util::SliceReference<'static, u8>> {
    FeatureFlagSliceType::<slice_util::SliceReference<'static, u8>>(item.0.freeze_dry())
}

pub fn write_header(header_progress: &mut usize,
                    window_size: u8,
                    output: &mut[u8],
                    output_offset:&mut usize) -> BrotliResult {
        let bytes_avail = output.len() - *output_offset;
        if bytes_avail + *header_progress < interface::HEADER_LENGTH {
            output.split_at_mut(*output_offset).1.clone_from_slice(
                &make_header(window_size)[*header_progress..
                                              (*header_progress + bytes_avail)]);
            *output_offset += bytes_avail;
            *header_progress += bytes_avail;
            return BrotliResult::NeedsMoreOutput;
        }
        output[*output_offset..(*output_offset + interface::HEADER_LENGTH - *header_progress)].clone_from_slice(
                &make_header(window_size)[*header_progress..]);
        *output_offset += interface::HEADER_LENGTH - *header_progress;
        *header_progress = interface::HEADER_LENGTH;
        BrotliResult::ResultSuccess

}

impl<DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>, AllocU8:Allocator<u8>, AllocU32:Allocator<u32>, AllocCDF2:Allocator<probability::CDF2>, AllocCDF16:Allocator<interface::DefaultCDF16>> 
    DivansCompressor<DefaultEncoder, AllocU8, AllocU32, AllocCDF2, AllocCDF16> {
    fn flush_freeze_dried_cmds(&mut self, output: &mut [u8], output_offset: &mut usize) -> brotli::BrotliResult {
        if self.freeze_dried_cmd_start != self.freeze_dried_cmd_end { // we have some freeze dried items
            let thawed_buffer = thaw_commands(&self.freeze_dried_cmd_array[..], self.cmd_assembler.ring_buffer.slice(),
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
        *freeze_dried_cmd_end = input.len();
        for (frozen, leftover) in freeze_dried_cmd_array.split_at_mut(input.len()).0.iter_mut().zip(input.iter()) {
            *frozen = match *leftover {
                Command::Literal(ref lit) => {
                    Command::Literal(LiteralCommand::<slice_util::SliceReference<'static, u8>> {
                        data: lit.data.freeze_dry(),
                        prob: freeze_dry(&lit.prob),
                    })
                },
                Command::PredictionMode(ref pm) => {
                    Command::PredictionMode(PredictionModeContextMap::<slice_util::SliceReference<'static, u8>> {
                        literal_prediction_mode: pm.literal_prediction_mode,
                        literal_context_map: pm.literal_context_map.freeze_dry(),
                        distance_context_map: pm.literal_context_map.freeze_dry(),
                    })
                },
                Command::Copy(ref c) => {
                    Command::Copy(*c)
                }
                Command::Dict(ref d) => {
                    Command::Dict(*d)
                }
                Command::BlockSwitchLiteral(ref l) => {
                    Command::BlockSwitchLiteral(*l)
                }
                Command::BlockSwitchCommand(ref c) => {
                    Command::BlockSwitchCommand(*c)
                }
                Command::BlockSwitchDistance(ref d) => {
                    Command::BlockSwitchDistance(*d)
                }
            };
        }
    }
    pub fn get_m8(&mut self) -> &mut AllocU8 {
       self.codec.get_m8()
    }
    pub fn free(mut self) -> (AllocU8, AllocU32, AllocCDF2, AllocCDF16) {
        let (m8, mcdf2, mcdf16) = self.codec.free();
        self.cmd_assembler.free(&mut self.m32);
        (m8, self.m32, mcdf2, mcdf16)
    }

}


impl<DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocU32:Allocator<u32>,
     AllocCDF2:Allocator<probability::CDF2>,
     AllocCDF16:Allocator<interface::DefaultCDF16>> Compressor for DivansCompressor<DefaultEncoder,
                                                                                    AllocU8,
                                                                                    AllocU32,
                                                                                    AllocCDF2,
                                                                                    AllocCDF16> {
    fn encode(&mut self,
              input: &[u8],
              input_offset: &mut usize,
              output: &mut [u8],
              output_offset: &mut usize) -> BrotliResult {
        if self.header_progress != interface::HEADER_LENGTH {
            match write_header(&mut self.header_progress, self.window_size, output, output_offset) {
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
            let command_decode_ret = self.cmd_assembler.stream(input, input_offset,
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
                    if let BrotliResult::NeedsMoreInput = command_decode_ret {
                        return BrotliResult::NeedsMoreInput; // we've exhausted all commands and all input
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
        if self.header_progress != interface::HEADER_LENGTH {
            match write_header(&mut self.header_progress, self.window_size, output, output_offset) {
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
        if self.header_progress != interface::HEADER_LENGTH {
            match write_header(&mut self.header_progress, self.window_size, output, output_offset) {
                BrotliResult::ResultSuccess => {},
                res => return res,
            }
        }
        match self.flush_freeze_dried_cmds(output, output_offset) {
               BrotliResult::ResultFailure => return BrotliResult::ResultFailure, 
               BrotliResult::NeedsMoreOutput => return BrotliResult::NeedsMoreOutput,
               BrotliResult::NeedsMoreInput | BrotliResult::ResultSuccess => {},
        }
        loop {
            let mut temp_bs: [interface::Command<slice_util::SliceReference<u8>>;COMPRESSOR_CMD_BUFFER_SIZE] =
                [interface::Command::<slice_util::SliceReference<u8>>::default();COMPRESSOR_CMD_BUFFER_SIZE];
            let mut temp_cmd_offset = 0;
            let command_flush_ret = self.cmd_assembler.flush(&mut temp_bs[..], &mut temp_cmd_offset);
            match command_flush_ret {
                BrotliResult::ResultSuccess => {
                    if temp_cmd_offset == 0 {
                        break; // no output from the cmd_assembler, just plain flush the codec
                    }
                },
                BrotliResult::ResultFailure | BrotliResult::NeedsMoreInput => {
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
                BrotliResult::ResultSuccess | BrotliResult::NeedsMoreInput => {
                    assert_eq!(temp_cmd_offset, out_cmd_offset); // must have consumed all commands
                    if let BrotliResult::ResultSuccess = command_flush_ret {
                         break; // we've exhausted all commands and all input
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
        self.codec.flush(output, output_offset)
    }
}

