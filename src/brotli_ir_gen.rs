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
use core::marker::PhantomData;
use core::cmp::{min, max};
use super::probability::CDF16;
use super::brotli;
use super::mux::{Mux,DevNull};
use codec::io::DemuxerAndRingBuffer;
use codec::{CommandArray, CommandSliceArray};
pub use super::alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use super::interface::{BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType,
    LITERAL_PREDICTION_MODE_SIGN,
    LITERAL_PREDICTION_MODE_UTF8,
    LITERAL_PREDICTION_MODE_MSB6,
    LITERAL_PREDICTION_MODE_LSB6,
};

pub use super::cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec, StrideSelection, StructureSeeker};
use super::resizable_buffer::ResizableByteBuffer;
use super::interface;
use super::interface::{DivansOutputResult, DivansResult, ErrMsg};
use super::brotli::enc::encode::{BrotliEncoderStateStruct, BrotliEncoderCompressStream, BrotliEncoderOperation, BrotliEncoderIsFinished};
use super::brotli::enc::backward_references::BrotliEncoderMode;
use super::divans_compressor::write_header;
pub struct BrotliDivansHybridCompressor<SelectedCDF:CDF16,
                                        Parser:StructureSeeker,
                                        ChosenEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                            AllocU8:Allocator<u8>,
                            AllocCDF16:Allocator<SelectedCDF>,
                            AllocBrotli: brotli::enc::BrotliAlloc>
{
    brotli_encoder: BrotliEncoderStateStruct<AllocBrotli>,
    codec: DivansCodec<ChosenEncoder, EncoderSpecialization, DemuxerAndRingBuffer<AllocU8, DevNull<AllocU8>>, Mux<AllocU8>, SelectedCDF, AllocU8, AllocCDF16, Parser>,
    header_progress: usize,
    window_size: u8,
    brotli_data: ResizableByteBuffer<u8, AllocBrotli>,
    divans_data: ResizableByteBuffer<u8, AllocU8>,
    encoded_byte_offset: usize,
    opt: super::interface::DivansCompressorOptions,
}



impl<SelectedCDF:CDF16,
     Parser:StructureSeeker,
     ChosenEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<SelectedCDF>,
     AllocBrotli: brotli::enc::BrotliAlloc,
     > BrotliDivansHybridCompressor<SelectedCDF,
                                    Parser,
                                    ChosenEncoder,
                                    AllocU8,
                                    AllocCDF16,
                                    AllocBrotli> {
    pub fn get_m8(&mut self) -> &mut AllocU8 {
       self.codec.get_m8().unwrap().get_base_alloc()
    }
    #[cfg(not(feature="std"))]
    fn do_panic(_m:ErrMsg) {
        panic!("Internal Error With Compression Stage")
    }
    #[cfg(feature="std")]
    fn do_panic(m:ErrMsg) {
        panic!(m)
    }
    fn divans_encode_commands<Commands:CommandArray>(cmd:&Commands,
                                                          header_progress: &mut usize,
                                                          data:&mut ResizableByteBuffer<u8, AllocU8>,
                                                          codec: &mut DivansCodec<ChosenEncoder,
                                                                                  EncoderSpecialization,
                                                                                  DemuxerAndRingBuffer<AllocU8, DevNull<AllocU8>>,
                                                                                  Mux<AllocU8>,
                                                                                  SelectedCDF,
                                                                                  AllocU8,
                                                                                  AllocCDF16,
                                                                                  Parser>,
                                                          window_size: u8) {
        let mut cmd_offset = 0usize;
        loop {
            let ret: DivansResult;
            let mut output_offset = 0usize;
            {
                let output = data.checkout_next_buffer(codec.get_m8().as_mut().unwrap().get_base_alloc(),
                                                           Some(interface::HEADER_LENGTH + 256));
                if *header_progress != interface::HEADER_LENGTH {
                    match write_header(header_progress, window_size, output, &mut output_offset, codec.get_crc()) {
                        DivansOutputResult::Success => {},
                        _ => panic!("Unexpected failure writing header"),
                    }
                }
                let mut unused: usize = 0;
                ret = codec.encode_or_decode(&[],
                                             &mut unused,
                                             output,
                                             &mut output_offset,
                                             cmd,
                                             &mut cmd_offset);
            }
            match ret {
                DivansResult::Success | DivansResult::NeedsMoreInput => {
                    assert_eq!(cmd_offset, cmd.len());
                    data.commit_next_buffer(output_offset);
                    return;
                },
                DivansResult::Failure(m) => Self::do_panic(m),
                DivansResult::NeedsMoreOutput => {
                    data.commit_next_buffer(output_offset);
                }
            }
        }
    }
    fn internal_encode_stream(&mut self,
                              op: BrotliEncoderOperation,
                              input:&[u8], input_offset: &mut usize,
                              is_end: bool) -> interface::DivansResult {
        let mut nothing : Option<usize> = None;
        let mut cb_err:Result<(),ErrMsg> = Ok(());
        {
            let divans_data_ref = &mut self.divans_data;
            let divans_codec_ref = &mut self.codec;
            let header_progress_ref = &mut self.header_progress;
            let window_size = self.window_size;
            let opt = self.opt;
            let mut cb = |pm:&mut brotli::interface::PredictionModeContextMap<brotli::InputReferenceMut>,
                          a:&mut [brotli::interface::Command<brotli::SliceOffset>],
                          mb:brotli::InputPair,
                          mc:&mut AllocBrotli| {
                              let mut expanded_buffer  = <AllocBrotli as Allocator<brotli::enc::StaticCommand>>::AllocatedMemory::default();
                              {
                                  let final_cmd = if opt.divans_ir_optimizer != 0 {
                                      match super::ir_optimize::ir_optimize(pm, a, mb, divans_codec_ref, window_size, opt, mc, &mut  expanded_buffer) {
                                          Ok(buf) => buf,
                                          Err(e) => {cb_err = Err(e); return;},
                                      }
                                  } else {
                                      a
                                  };
                                  let tmp = Command::PredictionMode(PredictionModeContextMap::<brotli::InputReference>{
                                      literal_context_map:brotli::InputReference::from(&pm.literal_context_map),
                                      predmode_speed_and_distance_context_map:brotli::InputReference::from(&pm.predmode_speed_and_distance_context_map),
                                  });
                                  Self::divans_encode_commands(&CommandSliceArray(&[tmp]),
                                                               header_progress_ref,
                                                               divans_data_ref,
                                                               divans_codec_ref,
                                                               window_size);
                                  if final_cmd.len() != 0 {
                                      Self::divans_encode_commands(&ThawingSliceArray(final_cmd, mb),
                                                                   header_progress_ref,
                                                                   divans_data_ref,
                                                                   divans_codec_ref,
                                                                   window_size);
                                  }
                              }
                              if expanded_buffer.len() != 0 {
                                  <AllocBrotli as Allocator<brotli::enc::StaticCommand>>::free_cell(mc, expanded_buffer);
                              }
            };
            {
                let mut available_in = input.len() - *input_offset;
                if available_in == 0 && BrotliEncoderIsFinished(&mut self.brotli_encoder) != 0 {
                    return DivansResult::Success;
                }
                let mut available_out;
                let mut brotli_out_offset = 0usize;
                {
                    let brotli_buffer = self.brotli_data.checkout_next_buffer(&mut self.brotli_encoder.m8, Some(256));
                    available_out = brotli_buffer.len();

                    if BrotliEncoderCompressStream(&mut self.brotli_encoder,
                                                   op,
                                                   &mut available_in,
                                                   input,
                                                   input_offset,
                                                   &mut available_out,
                                                   brotli_buffer,
                                                   &mut brotli_out_offset,
                                                   &mut nothing,
                                                   &mut cb) <= 0 {
                        return DivansResult::Failure(ErrMsg::BrotliCompressStreamFail(0xff, 0xff));
                    }
                }
                self.brotli_data.commit_next_buffer(brotli_out_offset);
                if available_out != 0 && available_in == 0 && BrotliEncoderIsFinished(&mut self.brotli_encoder) == 0 {
                    return DivansResult::NeedsMoreInput;
                }
            }
        }
        if let Err(e) = cb_err {
            return DivansResult::Failure(e);
        }
        if is_end && BrotliEncoderIsFinished(&mut self.brotli_encoder) == 0 {
            return DivansResult::NeedsMoreOutput;
        }
        if is_end {
            loop { // flush divans coder
                let ret;
                let mut output_offset = 0usize;
                {
                    let mut output = self.divans_data.checkout_next_buffer(self.codec.get_m8().as_mut().unwrap().get_base_alloc(),
                                                                           Some(interface::HEADER_LENGTH + 256));
                    ret = self.codec.flush(&mut output, &mut output_offset);
                }
                self.divans_data.commit_next_buffer(output_offset);
                match ret {
                    DivansOutputResult::NeedsMoreOutput => {},
                    _ => return DivansResult::from(ret),
                }
            }
        } else {
            return DivansResult::NeedsMoreInput
        }
    }
    fn free_internal(&mut self) {
        self.brotli_data.free(&mut self.brotli_encoder.m8);
        self.divans_data.free(&mut self.codec.get_m8().as_mut().unwrap().get_base_alloc());
        brotli::enc::encode::BrotliEncoderDestroyInstance(&mut self.brotli_encoder);
    }
    pub fn free_ref(&mut self) {
        self.free_internal();
        self.codec.free_ref();
    }
    pub fn free(mut self) -> (AllocU8, AllocCDF16, AllocBrotli) {
        self.free_internal();
        let (m8, mcdf16) = self.codec.free();
        (m8, mcdf16, self.brotli_encoder.m8)
    }
}

impl<SelectedCDF:CDF16,
          Parser:StructureSeeker,
     ChosenEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
     AllocU8:Allocator<u8>,
     AllocCDF16:Allocator<SelectedCDF>,
     AllocBrotli: brotli::enc::BrotliAlloc,
     >
    Compressor for BrotliDivansHybridCompressor<SelectedCDF,
                                                Parser,
                                                 ChosenEncoder,
                                                 AllocU8,
                                                 AllocCDF16,
                                                 AllocBrotli> {
    fn encode(&mut self,
              input: &[u8],
              input_offset: &mut usize,
              _output: &mut [u8],
              _output_offset: &mut usize) -> DivansResult {
        match self.internal_encode_stream(BrotliEncoderOperation::BROTLI_OPERATION_PROCESS,
                                          input,
                                          input_offset,
                                          false) {
            DivansResult::NeedsMoreOutput => DivansResult::Failure(ErrMsg::BrotliInternalEncodeStreamNeedsOutputWithoutFlush),
            DivansResult::Failure(m) => DivansResult::Failure(m),
            DivansResult::Success | DivansResult::NeedsMoreInput => DivansResult::NeedsMoreInput,
        }
    }
    fn flush(&mut self,
             output: &mut [u8],
             output_offset: &mut usize) -> DivansOutputResult {
        let mut zero = 0usize;
        if self.header_progress != interface::HEADER_LENGTH {
            match write_header(&mut self.header_progress, self.window_size, output, output_offset, self.codec.get_crc()) {
                DivansOutputResult::Success => {},
                need => return need,
            }
        }
        loop {

            match self.internal_encode_stream(BrotliEncoderOperation::BROTLI_OPERATION_FINISH,
                                              &[],
                                              &mut zero,
                                              true) {
                DivansResult::Failure(m) => return DivansOutputResult::Failure(m),
                DivansResult::Success => break,
                DivansResult::NeedsMoreOutput => {},
                DivansResult::NeedsMoreInput => return DivansOutputResult::Failure(ErrMsg::BrotliIrGenFlushStreamNeedsInput),
            }
        }
        // we're in success area here
        let destination = output.split_at_mut(*output_offset).1;
        let src = self.divans_data.slice().split_at(self.encoded_byte_offset).1;
        let copy_len = min(src.len(), destination.len());
        destination.split_at_mut(copy_len).0.clone_from_slice(src.split_at(copy_len).0);
        *output_offset += copy_len;
        self.encoded_byte_offset += copy_len;
        if self.encoded_byte_offset == self.divans_data.len() {
            return DivansOutputResult::Success;
        }
        DivansOutputResult::NeedsMoreOutput
    }
    fn encode_commands<SliceType:SliceWrapper<u8>+Default>(&mut self,
                                                           input:&[Command<SliceType>],
                                                           input_offset : &mut usize,
                                                           output :&mut[u8],
                                                           output_offset: &mut usize) -> DivansOutputResult {
        if self.header_progress != interface::HEADER_LENGTH {
            match write_header(&mut self.header_progress, self.window_size, output, output_offset, self.codec.get_crc()) {
                DivansOutputResult::Success => {},
                res => return res,
            }
        }
        let mut unused: usize = 0;
        match self.codec.encode_or_decode(&[],
                                          &mut unused,
                                          output,
                                          output_offset,
                                          &CommandSliceArray(input),
                                          input_offset) {
            DivansResult::Success | DivansResult::NeedsMoreInput => DivansOutputResult::Success,
            DivansResult::Failure(m) => DivansOutputResult::Failure(m),
            DivansResult::NeedsMoreOutput => DivansOutputResult::NeedsMoreOutput,
        }
    }
}

pub struct BrotliDivansHybridCompressorFactory<AllocU8:Allocator<u8>,
                                               AllocCDF16: Allocator<interface::DefaultCDF16>,
                                               AllocBrotli: brotli::enc::BrotliAlloc, Parser: StructureSeeker> {
    p1: PhantomData<AllocU8>,
    p3: PhantomData<AllocCDF16>,
    ph: PhantomData<AllocBrotli>,
    pp: PhantomData<Parser>,
}

impl<AllocU8:Allocator<u8>,
     AllocU32:Allocator<u32>,
     AllocCDF16:Allocator<interface::DefaultCDF16>,
     AllocBrotli: brotli::enc::BrotliAlloc,
     Parser:StructureSeeker,
     > interface::DivansCompressorFactory<AllocU8, AllocU32, AllocCDF16>
    for BrotliDivansHybridCompressorFactory<AllocU8, AllocCDF16,
                                            AllocBrotli, Parser> {
    type DefaultEncoder = DefaultEncoderType!();
       type ConstructedCompressor = BrotliDivansHybridCompressor<interface::DefaultCDF16,
                                                               Parser,
                                                               Self::DefaultEncoder,
                                                               AllocU8,
                                                               AllocCDF16,
                                                               AllocBrotli>;
      type AdditionalArgs = AllocBrotli;
        fn new(mut m8: AllocU8, _m32: AllocU32, mcdf16:AllocCDF16,
               opt: super::interface::DivansCompressorOptions,
               additional_args: Self::AdditionalArgs) -> Self::ConstructedCompressor {
        let window_size = min(24, max(10, opt.window_size.unwrap_or(22)));
        let cmd_enc = Self::DefaultEncoder::new(&mut m8);
        let lit_enc = Self::DefaultEncoder::new(&mut m8);
        let mut ret = Self::ConstructedCompressor {
             brotli_data: ResizableByteBuffer::<u8, AllocBrotli>::new(),
             divans_data: ResizableByteBuffer::<u8, AllocU8>::new(),
             encoded_byte_offset:0, 
             brotli_encoder: brotli::enc::encode::BrotliEncoderCreateInstance(additional_args),
            codec:DivansCodec::<Self::DefaultEncoder, EncoderSpecialization, DemuxerAndRingBuffer<AllocU8, DevNull<AllocU8>>, Mux<AllocU8>, interface::DefaultCDF16, AllocU8, AllocCDF16, Parser>::new(
                m8,
                mcdf16,
                cmd_enc,
                lit_enc,
                EncoderSpecialization::new(),
                DemuxerAndRingBuffer::<AllocU8, DevNull<AllocU8>>::default(),
                window_size as usize,
                opt.dynamic_context_mixing.unwrap_or(0),
                opt.prior_depth,
                opt.literal_adaptation,
                opt.use_context_map,
                opt.force_stride_value,
                false,
            ),
            opt:opt,
            header_progress: 0,
            window_size: window_size as u8,
        };
        if let Some(prediction_mode) = opt.force_literal_context_mode {
            brotli::enc::encode::BrotliEncoderSetParameter(
                &mut ret.brotli_encoder,
                brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_MODE,
                match prediction_mode.0 {
                    LITERAL_PREDICTION_MODE_LSB6 => BrotliEncoderMode::BROTLI_FORCE_LSB_PRIOR as u32,
                    LITERAL_PREDICTION_MODE_MSB6 => BrotliEncoderMode::BROTLI_FORCE_MSB_PRIOR as u32,
                    LITERAL_PREDICTION_MODE_UTF8 => BrotliEncoderMode::BROTLI_FORCE_UTF8_PRIOR as u32,
                    LITERAL_PREDICTION_MODE_SIGN => BrotliEncoderMode::BROTLI_FORCE_SIGNED_PRIOR as u32,
                    _ => BrotliEncoderMode::BROTLI_FORCE_SIGNED_PRIOR as u32,
                });
        }
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_LGWIN,
                                                       window_size as u32);
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_LGBLOCK,
                                                       opt.lgblock.unwrap_or(18));
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_QUALITY,
                                                       u32::from(opt.quality.unwrap_or(10)));
        if opt.q9_5 {
            brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_Q9_5,
                                                       1);
        }
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_METABLOCK_CALLBACK,
                                                       1);
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_CDF_ADAPTATION_DETECTION,
                                                       u32::from(opt.speed_detection_quality.unwrap_or(0)));
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_STRIDE_DETECTION_QUALITY,
                                                       u32::from(opt.stride_detection_quality.unwrap_or(0)));
        if let Some(literal_byte_score) = opt.brotli_literal_byte_score {
            brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                           brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_LITERAL_BYTE_SCORE,
                                                           literal_byte_score);
        }
        
        brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                       brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_PRIOR_BITMASK_DETECTION,
                                                       u32::from(opt.prior_bitmask_detection));
        if let Some(speed) = opt.literal_adaptation {

            brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                           brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_CM_SPEED,
                                                           speed[3].inc() as u32);
            brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                           brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_CM_SPEED_MAX,
                                                           speed[3].lim() as u32);
            brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                           brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_CM_SPEED_LOW,
                                                           speed[2].inc() as u32);
            brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                           brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_CM_SPEED_LOW_MAX,
                                                           speed[2].lim() as u32);
            brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                           brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_SPEED,
                                                           speed[1].inc() as u32);
            brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                           brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_SPEED_MAX,
                                                           speed[1].lim() as u32);
            brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                           brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_SPEED_LOW,
                                                           speed[0].inc() as u32);
            brotli::enc::encode::BrotliEncoderSetParameter(&mut ret.brotli_encoder,
                                                           brotli::enc::encode::BrotliEncoderParameter::BROTLI_PARAM_SPEED_LOW_MAX,
                                                           speed[0].lim() as u32);
        }
        ret
    }
}


struct ThawingSliceArray<'a>(&'a [brotli::interface::Command<brotli::SliceOffset>],
                             brotli::InputPair<'a>);

impl<'a> CommandArray for ThawingSliceArray<'a> {
    fn get_input_command(&self, offset:usize) -> Command<brotli::InputReference> {
        brotli::thaw_pair(&self.0[offset], &self.1)
    }
    fn len(&self) -> usize {
        self.0.len()
    }
}
