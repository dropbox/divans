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
use divans;
use core::cmp;
use std::io::{self,Write, BufReader};

use super::ItemVecAllocator;
use super::ItemVec;
use super::brotli_decompressor::BrotliResult;
use super::alloc::{Allocator, SliceWrapperMut, SliceWrapper};

use divans::Command;
use divans::FeatureFlagSliceType;
use divans::LiteralCommand;
use divans::LiteralPredictionModeNibble;
use divans::LiteralBlockSwitch;
use divans::PredictionModeContextMap;
use divans::Compressor;
use divans::DivansCompressorFactory;
use divans::DivansCompressorFactoryStruct;
use divans::Speed;


#[cfg(feature="benchmark")]
extern crate test;
#[cfg(feature="benchmark")]
use self::test::Bencher;

pub struct LimitedBuffer<'a> {
  pub data: &'a mut [u8],
  pub write_offset: usize,
  pub read_offset: usize,
}

impl<'a> LimitedBuffer<'a> {
  pub fn new(buf: &'a mut [u8]) -> Self {
    LimitedBuffer {
        data: buf,
        write_offset: 0,
        read_offset: 0,
    }
  }
}
impl<'a> LimitedBuffer<'a> {
    fn reset(&mut self) {
        self.write_offset = 0;
        self.read_offset = 0;
        self.data.split_at_mut(32).0.clone_from_slice(&[0u8;32]); // clear the first 256 bits
    }
    fn reset_read(&mut self) {
        self.read_offset = 0;
    }
    fn written(&self) -> &[u8] {
        &self.data[..self.write_offset]
    }
}
impl<'a> io::Read for LimitedBuffer<'a> {
  fn read(self: &mut Self, buf: &mut [u8]) -> io::Result<usize> {
    let bytes_to_read = cmp::min(buf.len(), self.data.len() - self.read_offset);
    if bytes_to_read > 0 {
      buf[0..bytes_to_read].clone_from_slice(&self.data[self.read_offset..
                                              self.read_offset + bytes_to_read]);
    }
    self.read_offset += bytes_to_read;
    return Ok(bytes_to_read);
  }
}

impl<'a> io::Write for LimitedBuffer<'a> {
  fn write(self: &mut Self, buf: &[u8]) -> io::Result<usize> {
      let bytes_to_write = cmp::min(buf.len(), self.data.len() - self.write_offset);
      if bytes_to_write > 0 {
          self.data[self.write_offset..self.write_offset + bytes_to_write].clone_from_slice(
              &buf[..bytes_to_write]);
      } else {
          return Err(io::Error::new(io::ErrorKind::WriteZero, "OutOfBufferSpace"));
      }
      self.write_offset += bytes_to_write;
      Ok(bytes_to_write)
  }
  fn flush(self: &mut Self) -> io::Result<()> {
    return Ok(());
  }
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


fn bench_with_ir<Run: Runner,
                 TS: TestSelection>(buffer_size: usize,
                                    ts: TS,
                                    ratio: f64,
                                    runner: &mut Run,
                                    raw_file: &[u8],
                                    raw_bytes: usize,
                                    ir_file: &[u8],
                                    ir_bytes: usize) {
    let mut m8 = ItemVecAllocator::<u8>::default();
    let mut input_buffer = m8.alloc_cell(raw_bytes);
    let mut cmd_ir_buffer = m8.alloc_cell(ir_bytes);
    let mut rt_backing_buffer = m8.alloc_cell(raw_bytes);
    let mut dv_backing_buffer = m8.alloc_cell(raw_bytes);
    let mut rt_buffer = LimitedBuffer::new(rt_backing_buffer.slice_mut());
    for (index, val) in cmd_ir_buffer.slice_mut().iter_mut().enumerate() {
        *val = ir_file[index % ir_file.len()];
    }
    for (index, val) in input_buffer.slice_mut().iter_mut().enumerate() {
        *val = raw_file[index % raw_file.len()];
    }
    let ir_buffer = LimitedBuffer::new(cmd_ir_buffer.slice_mut());
    let mut buf_ir = BufReader::new(ir_buffer);
    let mut dv_buffer = LimitedBuffer::new(dv_backing_buffer.slice_mut());
    let mixing_mode = if ts.adaptive_context_mixing() {
        2
    } else {
        match ts.stride_selection() {
            divans::StrideSelection::PriorDisabled => 0,
            _ => if ts.use_context_map() {
                1
            } else {
                0
            }
        }
    };
    super::compress_ir(&mut buf_ir,
                       &mut dv_buffer,
                       Some(mixing_mode),
                       Some(Speed::MUD),
                       ts.use_context_map(),
                       ts.stride_selection()).unwrap();
    {
        let mut decompress_lambda = || {
            dv_buffer.reset_read();
            rt_buffer.reset();
            super::decompress(&mut dv_buffer, &mut rt_buffer, buffer_size).unwrap();
            let actual_ratio =  dv_buffer.written().len() as f64 / input_buffer.slice().len() as f64;
            if !(actual_ratio <= ratio) {
                println!("Failed: actual buffer length {} dv_buffer size: {}", input_buffer.slice().len(), dv_buffer.written().len());
            }
            assert!(actual_ratio <= ratio);
        };
        runner.iter(&mut decompress_lambda);
    }
    assert_eq!(rt_buffer.written(), input_buffer.slice());
}


fn bench_no_ir<Run: Runner,
               TS: TestSelection>(buffer_size: usize,
                                  ts: TS,
                                  ratio: f64,
                                  measure_compress: bool,
                                  measure_decompress: bool,
                                  runner: &mut Run) {
    let mut m8 = ItemVecAllocator::<u8>::default();
    let mut input_buffer = m8.alloc_cell(ts.size());
    let mut cmd_data_buffer = m8.alloc_cell(ts.size());
    let mut temp_buffer = m8.alloc_cell(buffer_size);
    let mut dv_backing_buffer = m8.alloc_cell(input_buffer.slice().len() + 16);
    let mut rt_backing_buffer = m8.alloc_cell(input_buffer.slice().len() + 16);
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
            high_entropy:false,
        }),
    ];
    let mut dv_buffer = LimitedBuffer::new(dv_backing_buffer.slice_mut());//UnlimitedBuffer::new(&[]);//LimitedBuffer::new(dv_backing_buffer.slice_mut());
    let mut rt_buffer = LimitedBuffer::new(rt_backing_buffer.slice_mut());//UnlimitedBuffer::new(&[]);//LimitedBuffer::new(rt_backing_buffer.slice_mut());
    let mut compress_or_decompress_lambda = |compress:bool| {
        if compress {
            dv_buffer.reset();
            let mut encode_state =DivansCompressorFactoryStruct::<ItemVecAllocator<u8>,
                                                                  ItemVecAllocator<divans::CDF2>,
                                                                  ItemVecAllocator<divans::DefaultCDF16>>::new(
                ItemVecAllocator::<u8>::default(),
                ItemVecAllocator::<u32>::default(),
                ItemVecAllocator::<divans::CDF2>::default(),
                ItemVecAllocator::<divans::DefaultCDF16>::default(),
                22, // window_size 
                ts.adaptive_context_mixing() as u8 * 2,
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
        } else {
            dv_buffer.reset_read();
            rt_buffer.reset();
            super::decompress(&mut dv_buffer, &mut rt_buffer, buffer_size).unwrap();
            assert_eq!(rt_buffer.written(), input_buffer.slice());
            let actual_ratio =  dv_buffer.written().len() as f64 / input_buffer.slice().len() as f64;
            if !(actual_ratio <= ratio) {
                println!("Failed: actual buffer length {} dv_buffer size: {}", input_buffer.slice().len(), dv_buffer.written().len());
            }
            assert!(actual_ratio <= ratio);
        }
    };
    if !measure_compress {
        compress_or_decompress_lambda(true);
    }
    runner.iter(&mut || {
        if measure_compress {
            compress_or_decompress_lambda(true);
        }
        if measure_decompress {
            compress_or_decompress_lambda(false);
        }
    });
    if !measure_decompress {
        compress_or_decompress_lambda(false);
    }
    // item_vec are reclaimed automatically, no free required
}

#[test]
fn test_raw_literal_stream() {
    bench_no_ir(65536,
                TestContextMixing{size:1024 * 1024 / 10},
                0.025,
                true,
                true,
                &mut Passthrough{});
}

#[test]
fn test_raw_adaptive_literal_stream() {
    bench_no_ir(65536,
                TestAdapt{size:1024 * 1024 / 10},
                0.29,
                true,
                true,
                &mut Passthrough{});
}


#[test]
fn test_raw_ir_literal_stream() {
    let raw_file = include_bytes!("../../testdata/random_then_unicode");
    let ir = include_bytes!("../../testdata/random_then_unicode.ir");
    bench_with_ir(65536,
                  TestContextMixing{size:1024 * 1024},
                  0.6,
                  &mut Passthrough{},
                  &raw_file[..],
                  1048682,
                  &ir[..],
                  3321851,
                  );
}

#[cfg(feature="benchmark")]
#[bench]
fn bench_ir_decode_context_mixing_1024k(b: &mut Bencher) {
    let raw_file = include_bytes!("../../testdata/random_then_unicode");
    let ir = include_bytes!("../../testdata/random_then_unicode.ir");
    bench_with_ir(65536,
                  TestContextMixing{size:1024 * 1024},
                  0.6,
                  &mut BenchmarkPassthrough(b),
                  &raw_file[..],
                  1048682,
                  &ir[..],
                  3321851,
                  );
}

#[cfg(feature="benchmark")]
#[bench]
fn bench_ir_decode_context_pure_average_1024k(b: &mut Bencher) {
    let raw_file = include_bytes!("../../testdata/random_then_unicode");
    let ir = include_bytes!("../../testdata/random_then_unicode.ir");
    bench_with_ir(65536,
                  TestContextMixingPureAverage{size:1024 * 1024},
                  0.6,
                  &mut BenchmarkPassthrough(b),
                  &raw_file[..],
                  1048682,
                  &ir[..],
                  3321851,
                  );
}

#[cfg(feature="benchmark")]
#[bench]
fn bench_ir_decode_model_adapt_1024k(b: &mut Bencher) {
    let raw_file = include_bytes!("../../testdata/random_then_unicode");
    let ir = include_bytes!("../../testdata/random_then_unicode.ir");
    bench_with_ir(65536,
                  TestAdapt{size:1024 * 1024},
                  0.6,
                  &mut BenchmarkPassthrough(b),
                  &raw_file[..],
                  1048682,
                  &ir[..],
                  3321851,
                  );
}

#[cfg(feature="benchmark")]
#[bench]
fn bench_ir_decode_simple_1024k(b: &mut Bencher) {
    let raw_file = include_bytes!("../../testdata/random_then_unicode");
    let ir = include_bytes!("../../testdata/random_then_unicode.ir");
    bench_with_ir(65536,
                  TestSimple{size:1024 * 1024},
                  0.6,
                  &mut BenchmarkPassthrough(b),
                  &raw_file[..],
                  1048682,
                  &ir[..],
                  3321851,
                  );
}

#[cfg(feature="benchmark")]
#[bench]
fn bench_e2e_decode_context_mixing_100k(b: &mut Bencher) {
    bench_no_ir(65536,
                TestContextMixing{size:1024 * 1024 / 10},
                0.025,
                false,
                true,
                &mut BenchmarkPassthrough(b));

}
#[cfg(feature="benchmark")]
#[bench]
fn bench_e2e_decode_context_pure_average_100k(b: &mut Bencher) {
    bench_no_ir(65536,
                TestContextMixingPureAverage{size:1024 * 1024 / 10},
                0.17,
                false,
                true,
                &mut BenchmarkPassthrough(b));

}
#[cfg(feature="benchmark")]
#[bench]
fn bench_e2e_decode_model_adapt_100k(b: &mut Bencher) {
    bench_no_ir(65536,
                TestAdapt{size:1024 * 1024 / 10},
                0.29,
                false,
                true,
                &mut BenchmarkPassthrough(b));

}
#[cfg(feature="benchmark")]
#[bench]
fn bench_e2e_decode_simple_100k(b: &mut Bencher) {
    bench_no_ir(65536,
                TestSimple{size:1024 * 1024 / 10},
                0.03,
                false,
                true,
                &mut BenchmarkPassthrough(b));

}

#[cfg(feature="benchmark")]
#[bench]
fn bench_e2e_roundtrip_context_mixing_100k(b: &mut Bencher) {
    bench_no_ir(65536,
                TestContextMixing{size:1024 * 1024 / 10},
                0.025,
                true,
                true,
                &mut BenchmarkPassthrough(b));

}
#[cfg(feature="benchmark")]
#[bench]
fn bench_e2e_roundtrip_context_pure_average_100k(b: &mut Bencher) {
    bench_no_ir(65536,
                TestContextMixingPureAverage{size:1024 * 1024 / 10},
                0.17,
                true,
                true,
                &mut BenchmarkPassthrough(b));

}
#[cfg(feature="benchmark")]
#[bench]
fn bench_e2e_roundtrip_model_adapt_100k(b: &mut Bencher) {
    bench_no_ir(65536,
                TestAdapt{size:1024 * 1024 / 10},
                0.29,
                true,
                true,
                &mut BenchmarkPassthrough(b));

}
#[cfg(feature="benchmark")]
#[bench]
fn bench_e2e_roundtrip_simple_100k(b: &mut Bencher) {
    bench_no_ir(65536,
                TestSimple{size:1024 * 1024 / 10},
                0.03,
                true,
                true,
                &mut BenchmarkPassthrough(b));

}
