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

#![cfg(test)]
extern crate core;
use std::io;
use divans;
use std::io::Write;
use super::ItemVecAllocator;
use super::ItemVec;
use super::brotli_decompressor::BrotliResult;
use super::brotli_decompressor::BrotliDecompressStream;
use super::brotli_decompressor::BrotliState;
use super::brotli_decompressor::HuffmanCode;
use super::util::HeapAllocator;
use super::alloc::{Allocator, SliceWrapperMut, SliceWrapper};
use super::integration_test::UnlimitedBuffer;

use divans::Command;
use divans::FeatureFlagSliceType;
use divans::LiteralCommand;
use divans::LiteralPredictionModeNibble;
use divans::LiteralBlockSwitch;
use divans::PredictionModeContextMap;
use divans::Compressor;
use divans::DivansCompressorFactory;
use divans::DivansCompressorFactoryStruct;


#[cfg(feature="benchmark")]
extern crate test;
#[cfg(feature="benchmark")]
use self::test::Bencher;



pub fn brotli_decompress_internal(brotli_file : &[u8]) -> Result<Box<[u8]>, io::Error> {
  let mut brotli_state =
      BrotliState::new(HeapAllocator::<u8> { default_value: 0 },
                       HeapAllocator::<u32> { default_value: 0 },
                       HeapAllocator::<HuffmanCode> { default_value: HuffmanCode::default() });
  let buffer_limit = 65536;
  let mut buffer = brotli_state.alloc_u8.alloc_cell(buffer_limit);
  let mut available_out: usize = buffer.slice().len();

  let mut available_in: usize = brotli_file.len();
  let mut input_offset: usize = 0;
  let mut output_offset: usize = 0;
  let mut uncompressed_file_from_brotli = UnlimitedBuffer::new(&[]);
  loop {
    let mut written = 0usize;
    let result = BrotliDecompressStream(&mut available_in,
                                    &mut input_offset,
                                    brotli_file,
                                    &mut available_out,
                                    &mut output_offset,
                                    buffer.slice_mut(),
                                    &mut written,
                                    &mut brotli_state);
    match result {
      BrotliResult::NeedsMoreInput => {
        panic!("File should have been in brotli format") 
      }
      BrotliResult::NeedsMoreOutput => {
        try!(uncompressed_file_from_brotli.write_all(&buffer.slice()[..output_offset]));
        output_offset = 0;
        available_out = buffer.slice().len();
      }
      BrotliResult::ResultSuccess => {
         try!(uncompressed_file_from_brotli.write_all(&buffer.slice()[..output_offset]));
         break;
      },
      BrotliResult::ResultFailure => panic!("FAILURE"),
    }
  }
  brotli_state.BrotliStateCleanup();
  
  Ok(uncompressed_file_from_brotli.data.into_boxed_slice())
}

pub fn divans_decompress_internal(mut brotli_file : &[u8]) -> Result<Box<[u8]>, io::Error> {
  let mut uncompressed_file_from_divans = UnlimitedBuffer::new(&[]);
  try!(super::recode(&mut brotli_file,
                &mut uncompressed_file_from_divans));
  Ok(uncompressed_file_from_divans.data.into_boxed_slice())
}

#[test]
fn test_ends_with_truncated_dictionary() {
   let raw_file = brotli_decompress_internal(include_bytes!("../../testdata/ends_with_truncated_dictionary.br")).unwrap();
   let div_raw = divans_decompress_internal(include_bytes!("../../testdata/ends_with_truncated_dictionary.ir")).unwrap();
   assert_eq!(raw_file.len(), div_raw.len());
   assert_eq!(raw_file, div_raw);
}
#[test]
fn test_random_then_unicode() {
   let raw_file = brotli_decompress_internal(include_bytes!("../../testdata/random_then_unicode.br")).unwrap();
   let div_input = brotli_decompress_internal(include_bytes!("../../testdata/random_then_unicode.ir.br")).unwrap();
   let div_raw = divans_decompress_internal(&*div_input).unwrap();
   assert_eq!(raw_file.len(), div_raw.len());
   assert_eq!(raw_file, div_raw);
}
#[test]
fn test_alice29() {
   let raw_file = brotli_decompress_internal(include_bytes!("../../testdata/alice29.br")).unwrap();
   let div_input = brotli_decompress_internal(include_bytes!("../../testdata/alice29-priors.ir.br")).unwrap();
   let div_raw = divans_decompress_internal(&*div_input).unwrap();
   assert_eq!(raw_file.len(), div_raw.len());
   assert_eq!(raw_file, div_raw);
}
#[test]
fn test_asyoulik() {
   let raw_file = brotli_decompress_internal(include_bytes!("../../testdata/asyoulik.br")).unwrap();
   let div_input = brotli_decompress_internal(include_bytes!("../../testdata/asyoulik.ir.br")).unwrap();
   assert_eq!(div_input.len(), 541890);
   let div_raw = divans_decompress_internal(&*div_input).unwrap();
   assert_eq!(raw_file.len(), div_raw.len());
   assert_eq!(raw_file, div_raw);
}

fn init_shuffle_384(src: &mut [u8]) -> u8 {
    let shuffled = [133, 240, 232, 124, 145, 29, 201, 207, 244, 226, 199, 176, 13, 173, 98, 179,
                    247, 69, 167, 6, 41, 117, 131, 44, 158, 38, 139, 253, 71, 250, 1, 101,
                    126, 65, 113, 57, 25, 97, 56, 16, 87, 64, 47, 138, 150, 212, 155, 0,
                    89, 118, 218, 68, 241, 77, 49, 112, 142, 143, 245, 48, 12, 152, 14, 195,
                    234, 95, 185, 37, 108, 137, 55, 63, 81, 120, 107, 34, 11, 52, 96, 111,
                    127, 189, 35, 223, 249, 221, 23, 154, 242, 136, 93, 141, 3, 84, 99, 248,
                    206, 62, 134, 211, 51, 216, 162, 61, 183, 72, 198, 40, 122, 202, 190, 163,
                    180, 171, 153, 159, 166, 186, 164, 210, 91, 165, 213, 30, 15, 33, 27, 172,
                    104, 121, 147, 219, 140, 36, 4, 28, 43, 45, 102, 24, 5, 168, 188, 114, 255,
                    160, 209, 181, 21, 182, 130, 254, 214, 83, 170, 82, 105, 187, 192, 156, 26,
                    196, 184, 54, 116, 46, 228, 115, 19, 76, 169, 225, 32, 10, 193, 60, 215, 103,
                    22, 42, 144, 80, 161, 78, 17, 94, 2, 31, 18, 203, 129, 20, 9, 227, 246, 224,
                    229, 135, 231, 73, 66, 125, 230, 119, 151, 67, 86, 205, 128, 174, 243, 74,
                    123, 92, 191, 110, 157, 106, 100, 70, 148, 237, 132, 109, 220, 53, 8, 197,
                    50, 175, 251, 208, 204, 79, 146, 149, 222, 178, 233, 58, 252, 217, 177, 7,
                    235, 236, 59, 194, 75, 85, 90, 238, 200, 239, 88, 39, 133, 240, 232, 124, 145,
                    29, 201, 207, 244, 226, 199, 176, 13, 173, 98, 179, 247, 69, 167, 6, 41, 117,
                    131, 44, 158, 38, 139, 253, 71, 250, 1, 101, 126, 65, 113, 57, 25, 97, 56,
                    16, 87, 64, 47, 138, 150, 212, 155, 0, 89, 118, 218, 68, 241, 77, 49, 112, 142,
                    143, 245, 48, 12, 152, 14, 195, 234, 95, 185, 37, 108, 137, 55, 63, 81, 120, 107,
                    34, 11, 52, 96, 111, 127, 189, 35, 223, 249, 221, 23, 154, 242, 136, 93, 141, 3,
                    84, 99, 248, 206, 62, 134, 211, 51, 216, 162, 61, 183, 72, 198, 40, 122, 202, 190,
                    163, 180, 171, 153, 159, 166, 186, 164, 210, 91, 165, 213, 30, 15, 33, 27, 172];
    for (s,v) in shuffled.iter().cycle().zip(src.iter_mut()) {
        *v = *s;
    }
    127
}
trait TestSelection : Clone + Copy {
    fn size(&self) -> usize;
    fn use_context_map(&self) -> bool;
    fn stride_selection(&self) -> divans::StrideSelection;
    fn adaptive_context_mixing(&self) -> bool;
    fn prediction_mode(&self) -> LiteralPredictionModeNibble;
}
#[derive(Clone, Copy)]
struct TestContextMixing{
    pub size: usize,
}
#[derive(Clone, Copy)]
struct TestContextMixingPureAverage{
    pub size: usize,
}
#[derive(Clone, Copy)]
struct TestAdapt{
    pub size: usize,
}
#[derive(Clone, Copy)]
struct TestSimple{
    pub size: usize,
}
impl TestSelection for TestContextMixing {
    fn size(&self) -> usize {self.size}
    fn use_context_map(&self) -> bool {true}
    fn stride_selection(&self) -> divans::StrideSelection {divans::StrideSelection::UseBrotliRec}
    fn adaptive_context_mixing(&self) -> bool {true}
    fn prediction_mode(&self) -> LiteralPredictionModeNibble {
        LiteralPredictionModeNibble::utf8()
    }
}

impl TestSelection for TestContextMixingPureAverage {
    fn size(&self) -> usize {self.size}
    fn use_context_map(&self) -> bool {true}
    fn stride_selection(&self) -> divans::StrideSelection {divans::StrideSelection::UseBrotliRec}
    fn adaptive_context_mixing(&self) -> bool {false}
    fn prediction_mode(&self) -> LiteralPredictionModeNibble {
        LiteralPredictionModeNibble::utf8()
    }
}

impl TestSelection for TestAdapt {
    fn size(&self) -> usize {self.size}
    fn use_context_map(&self) -> bool {true}
    fn stride_selection(&self) -> divans::StrideSelection {divans::StrideSelection::PriorDisabled}
    fn adaptive_context_mixing(&self) -> bool {false}
    fn prediction_mode(&self) -> LiteralPredictionModeNibble {
        LiteralPredictionModeNibble::lsb6()
    }
}

impl TestSelection for TestSimple {
    fn size(&self) -> usize {self.size}
    fn use_context_map(&self) -> bool {false}
    fn stride_selection(&self) -> divans::StrideSelection {divans::StrideSelection::Stride1}
    fn adaptive_context_mixing(&self) -> bool {false}
    fn prediction_mode(&self) -> LiteralPredictionModeNibble {
        LiteralPredictionModeNibble::lsb6()
    }
}

trait Runner {
    fn iter<Fn:FnMut()> (&mut self, cb: &mut Fn);
}

struct Passthrough {
}
impl Runner for Passthrough {
    fn iter<Fn:FnMut()> (&mut self, cb: &mut Fn) {
        cb()
    }
}

#[cfg(feature="benchmark")]
struct BenchmarkPassthrough<'a> (pub &'a mut Bencher);
#[cfg(feature="benchmark")]
impl<'a> Runner for BenchmarkPassthrough<'a> {
    fn iter<Fn:FnMut()> (&mut self, cb: &mut Fn) {
        self.0.iter(cb)
    }
    
}





fn bench_no_ir<Run: Runner,
               TS: TestSelection>(buffer_size: usize,
                                  ts: TS,
                                  ratio: f64,
                                  runner: &mut Run) {
    let mut m8 = ItemVecAllocator::<u8>::default();
    let mut input_buffer = m8.alloc_cell(ts.size());
    let mut cmd_data_buffer = m8.alloc_cell(ts.size());
    let mut temp_buffer = m8.alloc_cell(buffer_size);
    let mut cm = m8.alloc_cell(256);
    let mut dm = m8.alloc_cell(256);
    for (index, item) in cm.slice_mut().iter_mut().enumerate() {
        *item = (index & 63) as u8;
    }
    for (index, item) in dm.slice_mut().iter_mut().enumerate() {
        *item = (index & 63) as u8;
    }
    init_shuffle_384(input_buffer.slice_mut());
    cmd_data_buffer.slice_mut().clone_from_slice(input_buffer.slice());
    let ibuffer:[Command<ItemVec<u8>>;3] = [
        Command::PredictionMode(PredictionModeContextMap{
            literal_prediction_mode: ts.prediction_mode(),
            literal_context_map: cm,
            distance_context_map: dm,
        }),
        Command::BlockSwitchLiteral(LiteralBlockSwitch::new(1, 2)),
        Command::Literal(LiteralCommand{
            data:cmd_data_buffer,
            prob:FeatureFlagSliceType::<ItemVec<u8>>::default(),
        }),
    ];
    runner.iter(&mut || {
    let mut dv_buffer = UnlimitedBuffer::new(&[]);
    let mut encode_state =DivansCompressorFactoryStruct::<ItemVecAllocator<u8>,
                                                      ItemVecAllocator<divans::CDF2>,
                                                      ItemVecAllocator<divans::DefaultCDF16>>::new(
        ItemVecAllocator::<u8>::default(),
        ItemVecAllocator::<u32>::default(),
        ItemVecAllocator::<divans::CDF2>::default(),
        ItemVecAllocator::<divans::DefaultCDF16>::default(),
        22, // window_size 
        ts.adaptive_context_mixing() as u8,
        None, // speed
        ts.use_context_map(),
        ts.stride_selection(),
        (),
    );

    super::recode_cmd_buffer(&mut encode_state,
                              &ibuffer[..],
                              &mut dv_buffer,
                             temp_buffer.slice_mut()).unwrap();
    loop {
        let mut o_processed_index = 0;
        match encode_state.flush(temp_buffer.slice_mut(),
                                 &mut o_processed_index) {
            BrotliResult::ResultSuccess => {
                if o_processed_index != 0 {
                    dv_buffer.write_all(temp_buffer.slice_mut().split_at(o_processed_index).0).unwrap();
                }
                break;
            },
            BrotliResult::NeedsMoreOutput => {
                assert!(o_processed_index != 0);
                dv_buffer.write_all(temp_buffer.slice_mut().split_at(o_processed_index).0).unwrap();
            }
            _ => {
                panic!("Unreasonable demand: no input avail in this code path");
            }
        }
    }

    {
        let mut rt_buffer = UnlimitedBuffer::new(&[]);
        super::decompress(&mut dv_buffer, &mut rt_buffer, buffer_size).unwrap();
        assert_eq!(&rt_buffer.data[..], input_buffer.slice());
        let actual_ratio =  dv_buffer.data.len() as f64 / input_buffer.slice().len() as f64;
        if !(actual_ratio <= ratio) {
            println!("Failed: actual buffer length {} dv_buffer size: {}", input_buffer.slice().len(), dv_buffer.data.len());
        }
        assert!(actual_ratio <= ratio);
    }
    });
    // item_vec are reclaimed automatically, no free required
}

#[test]
fn test_raw_literal_stream() {
    bench_no_ir(65536,
                TestContextMixing{size:100000},
                0.025,
                &mut Passthrough{});
}


#[cfg(feature="benchmark")]
#[bench]
fn bench_roundtrip_context_mixing_100k(b: &mut Bencher) {
    bench_no_ir(65536,
                TestContextMixing{size:100000},
                0.025,
                &mut BenchmarkPassthrough(b));

}
