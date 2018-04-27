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
use core;
use core::marker::PhantomData;
use core::hash::Hasher;
use super::probability;

use super::raw_to_cmd;
use super::slice_util;
use super::alloc_util::RepurposingAlloc;
pub use super::alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use super::interface::{
    BlockSwitch,
    LiteralBlockSwitch,
    Command,
    Compressor,
    CopyCommand,
    Decompressor,
    DictCommand,
    LiteralCommand,
    Nop,
    NewWithAllocator,
    ArithmeticEncoderOrDecoder,
    LiteralPredictionModeNibble,
    PredictionModeContextMap,
    FeatureFlagSliceType,
    free_cmd,
    };

pub use super::cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec, StrideSelection, default_crc};
use super::interface;
use super::interface::{DivansOutputResult, DivansResult, ErrMsg};
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
    literal_context_map_backing: AllocU8::AllocatedMemory,
    prediction_mode_backing: AllocU8::AllocatedMemory,
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
     fn new(mut m8: AllocU8, mut m32: AllocU32, mcdf2:AllocCDF2, mcdf16:AllocCDF16,
            opts: super::interface::DivansCompressorOptions,
            _additional_args: ()) -> DivansCompressor<Self::DefaultEncoder, AllocU8, AllocU32, AllocCDF2, AllocCDF16> {
         let window_size = core::cmp::min(24, core::cmp::max(10, opts.window_size.unwrap_or(22)));
         let ring_buffer = m8.alloc_cell(1<<window_size);
         let prediction_mode_backing = m8.alloc_cell(interface::MAX_PREDMODE_SPEED_AND_DISTANCE_CONTEXT_MAP_SIZE);
         let literal_context_map = m8.alloc_cell(interface::MAX_LITERAL_CONTEXT_MAP_SIZE);
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
                window_size as usize,
                opts.dynamic_context_mixing.unwrap_or(0),
                opts.prior_depth,
                opts.literal_adaptation,
                opts.use_context_map,
                opts.force_stride_value,
                false,
            ),
            literal_context_map_backing: literal_context_map,
            prediction_mode_backing: prediction_mode_backing,
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
               pm.predmode_speed_and_distance_context_map = pm.predmode_speed_and_distance_context_map.thaw(ring_buffer);
           },
           Command::Dict(_) |
           Command::Copy(_) |
           Command::BlockSwitchCommand(_) |
           Command::BlockSwitchLiteral(_) |
           Command::BlockSwitchDistance(_) => {}
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

pub fn write_header<CRC:Hasher>(header_progress: &mut usize,
                                window_size: u8,
                                output: &mut[u8],
                                output_offset:&mut usize,
                                crc: &mut CRC) -> DivansOutputResult {
    let bytes_avail = output.len() - *output_offset;
    if bytes_avail + *header_progress < interface::HEADER_LENGTH {
        let to_write = &make_header(window_size)[*header_progress..
                                                 (*header_progress + bytes_avail)];
        crc.write(to_write);
        output.split_at_mut(*output_offset).1.clone_from_slice(
            to_write);
        *output_offset += bytes_avail;
        *header_progress += bytes_avail;
        return DivansOutputResult::NeedsMoreOutput;
    }
    let to_write = &make_header(window_size)[*header_progress..];
    output[*output_offset..(*output_offset + interface::HEADER_LENGTH - *header_progress)].clone_from_slice(
        to_write);
    crc.write(to_write);
    *output_offset += interface::HEADER_LENGTH - *header_progress;
    *header_progress = interface::HEADER_LENGTH;
    DivansOutputResult::Success

}

impl<DefaultEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>, AllocU8:Allocator<u8>, AllocU32:Allocator<u32>, AllocCDF2:Allocator<probability::CDF2>, AllocCDF16:Allocator<interface::DefaultCDF16>> 
    DivansCompressor<DefaultEncoder, AllocU8, AllocU32, AllocCDF2, AllocCDF16> {
    fn flush_freeze_dried_cmds(&mut self, output: &mut [u8], output_offset: &mut usize) -> interface::DivansOutputResult {
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
               DivansResult::Failure(m) => return DivansOutputResult::Failure(m),
               DivansResult::NeedsMoreInput | DivansResult::Success => {},
               DivansResult::NeedsMoreOutput => return DivansOutputResult::NeedsMoreOutput,
            }
        }
        DivansOutputResult::Success
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
                        high_entropy: lit.high_entropy,
                    })
                },
                Command::PredictionMode(ref pm) => {
                    Command::PredictionMode(PredictionModeContextMap::<slice_util::SliceReference<'static, u8>> {
                        literal_context_map: pm.literal_context_map.freeze_dry(),
                        predmode_speed_and_distance_context_map: pm.predmode_speed_and_distance_context_map.freeze_dry(),
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
    pub fn get_m8(&mut self) -> &mut RepurposingAlloc<u8, AllocU8> {
       self.codec.get_m8()
    }
    pub fn free_ref(&mut self) {
        self.cmd_assembler.free(&mut self.m32);
        self.codec.get_m8().free_cell(core::mem::replace(&mut self.literal_context_map_backing, AllocU8::AllocatedMemory::default()));
        self.codec.get_m8().free_cell(core::mem::replace(&mut self.prediction_mode_backing, AllocU8::AllocatedMemory::default()));
        self.codec.free_ref();
    }
    pub fn free(mut self) -> (AllocU8, AllocU32, AllocCDF2, AllocCDF16) {
        let (mut m8, mcdf2, mcdf16) = self.codec.free();
        self.cmd_assembler.free(&mut self.m32);
        m8.free_cell(core::mem::replace(&mut self.literal_context_map_backing, AllocU8::AllocatedMemory::default()));
        m8.free_cell(core::mem::replace(&mut self.prediction_mode_backing, AllocU8::AllocatedMemory::default()));
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
              output_offset: &mut usize) -> DivansResult {
        if self.header_progress != interface::HEADER_LENGTH {
            match write_header(&mut self.header_progress, self.window_size, output, output_offset,
                               self.codec.get_crc()) {
                DivansOutputResult::Success => {},
                res => return DivansResult::from(res),
            }
        }
        match self.flush_freeze_dried_cmds(output, output_offset) {
            DivansOutputResult::Success => {},
            res => return DivansResult::from(res),
        }
        let literal_context_map = self.literal_context_map_backing.slice_mut();
        let prediction_mode_backing = self.prediction_mode_backing.slice_mut();
        loop {
            let mut temp_bs: [interface::Command<slice_util::SliceReference<u8>>;COMPRESSOR_CMD_BUFFER_SIZE] =
                [interface::Command::<slice_util::SliceReference<u8>>::default();COMPRESSOR_CMD_BUFFER_SIZE];
            let mut temp_cmd_offset = 0;
            let command_decode_ret = self.cmd_assembler.stream(input, input_offset,
                                                               &mut temp_bs[..], &mut temp_cmd_offset,
                                                               literal_context_map, prediction_mode_backing);
            match command_decode_ret {
                DivansResult::NeedsMoreInput => {
                    if temp_cmd_offset == 0 {
                        // nothing to freeze dry, return
                        return DivansResult::NeedsMoreInput;
                    }
                },
                DivansResult::Success => return DivansResult::Failure(ErrMsg::AssemblerStreamReportsDone), // we are never done
                DivansResult::Failure(m) => return DivansResult::Failure(m),
                DivansResult::NeedsMoreOutput => {},
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
                DivansResult::NeedsMoreInput | DivansResult::Success => {
                    assert_eq!(temp_cmd_offset, out_cmd_offset); // must have consumed all commands
                    if let DivansResult::NeedsMoreInput = command_decode_ret {
                        return DivansResult::NeedsMoreInput; // we've exhausted all commands and all input
                    }
                },
                DivansResult::NeedsMoreOutput | DivansResult::Failure(_) => {
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
                                          output_offset: &mut usize) -> DivansOutputResult{
        if self.header_progress != interface::HEADER_LENGTH {
            match write_header(&mut self.header_progress, self.window_size, output, output_offset,
                               self.codec.get_crc()) {
                DivansOutputResult::Success => {},
                res => return res,
            }
        }
        let mut unused: usize = 0;
        match self.codec.encode_or_decode(&[],
                                    &mut unused,
                                    output,
                                    output_offset,
                                    input,
                                          input_offset) {
            DivansResult::Success | DivansResult::NeedsMoreInput => DivansOutputResult::Success,
            DivansResult::NeedsMoreOutput => DivansOutputResult::NeedsMoreOutput,
            DivansResult::Failure(m) => DivansOutputResult::Failure(m),
        }
    }
    fn flush(&mut self,
             output: &mut [u8],
             output_offset: &mut usize) -> DivansOutputResult {
        if self.header_progress != interface::HEADER_LENGTH {
            match write_header(&mut self.header_progress, self.window_size, output, output_offset,
                               self.codec.get_crc()) {
                DivansOutputResult::Success => {},
                res => return res,
            }
        }
        match self.flush_freeze_dried_cmds(output, output_offset) {
               DivansOutputResult::Success => {},
               res => return res,
        }
        loop {
            let literal_context_map_backing = self.literal_context_map_backing.slice_mut();
            let prediction_mode_backing = self.prediction_mode_backing.slice_mut();
            let mut temp_bs: [interface::Command<slice_util::SliceReference<u8>>;COMPRESSOR_CMD_BUFFER_SIZE] =
                [interface::Command::<slice_util::SliceReference<u8>>::default();COMPRESSOR_CMD_BUFFER_SIZE];
            let mut temp_cmd_offset = 0;
            let command_flush_ret = self.cmd_assembler.flush(&mut temp_bs[..], &mut temp_cmd_offset, literal_context_map_backing, prediction_mode_backing);
            match command_flush_ret {
                DivansOutputResult::Success => {
                    if temp_cmd_offset == 0 {
                        break; // no output from the cmd_assembler, just plain flush the codec
                    }
                },
                DivansOutputResult::Failure(m) => {
                    return DivansOutputResult::Failure(m); // we are never done
                },
                DivansOutputResult::NeedsMoreOutput => {},
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
                DivansResult::Success | DivansResult::NeedsMoreInput => {
                    assert_eq!(temp_cmd_offset, out_cmd_offset); // must have consumed all commands
                    if let DivansOutputResult::Success = command_flush_ret {
                         break; // we've exhausted all commands and all input
                    }
                },
                DivansResult::NeedsMoreOutput | DivansResult::Failure(_) => {
                    Self::freeze_dry(
                        &mut self.freeze_dried_cmd_array,
                        &mut self.freeze_dried_cmd_start,
                        &mut self.freeze_dried_cmd_end,
                        &temp_bs[out_cmd_offset..temp_cmd_offset]);
                    match codec_ret {
                        DivansResult::Success | DivansResult::NeedsMoreInput => return DivansOutputResult::Failure(
                            ErrMsg::WrongInternalEncoderState(0)),
                        DivansResult::NeedsMoreOutput => return DivansOutputResult::NeedsMoreOutput,
                        DivansResult::Failure(m) => return DivansOutputResult::Failure(m),
                    }
                }
            }
        }
        self.codec.flush(output, output_offset)
    }
}

