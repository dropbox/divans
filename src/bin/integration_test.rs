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

use std::io::BufReader;
use core::cmp;
use divans::{Speed, StrideSelection, DivansCompressorOptions, BrotliCompressionSetting};
#[cfg(feature="no-stdlib")]
pub static MULTI: bool = false;
#[cfg(not(feature="no-stdlib"))]
pub static MULTI: bool = true;
pub struct UnlimitedBuffer {
  pub data: Vec<u8>,
  pub read_offset: usize,
}

impl UnlimitedBuffer {
  pub fn new(buf: &[u8]) -> Self {
    let mut ret = UnlimitedBuffer {
      data: Vec::<u8>::new(),
      read_offset: 0,
    };
    ret.data.extend(buf);
    return ret;
  }
  #[allow(unused)]
  pub fn written(&self) -> &[u8] {
    &self.data[..]
  }
}

impl io::Read for UnlimitedBuffer {
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

impl io::Write for UnlimitedBuffer {
  fn write(self: &mut Self, buf: &[u8]) -> io::Result<usize> {
    self.data.extend(buf);
    return Ok(buf.len());
  }
  fn flush(self: &mut Self) -> io::Result<()> {
    return Ok(());
  }
}


pub fn divans_decompress_internal(mut brotli_file : &[u8]) -> Result<Box<[u8]>, io::Error> {
  let mut uncompressed_file_from_divans = UnlimitedBuffer::new(&[]);
  try!(super::recode(&mut brotli_file,
                &mut uncompressed_file_from_divans));
  Ok(uncompressed_file_from_divans.data.into_boxed_slice())
}

#[test]
fn test_ends_with_truncated_dictionary() {
   let raw_file = include_bytes!("../../testdata/ends_with_truncated_dictionary");
   let div_input = include_bytes!("../../testdata/ends_with_truncated_dictionary.ir");
   let div_raw = divans_decompress_internal(&*div_input).unwrap();
   assert_eq!(raw_file.len(), div_raw.len());
   assert_eq!(&raw_file[..], &div_raw[..]);
}
#[test]
fn test_random_then_unicode() {
   let raw_file = include_bytes!("../../testdata/random_then_unicode");
   let div_input = include_bytes!("../../testdata/random_then_unicode.ir");
   let div_raw = divans_decompress_internal(&*div_input).unwrap();
   assert_eq!(raw_file.len(), div_raw.len());
   assert_eq!(&raw_file[..], &div_raw[..]);
}
#[test]
fn test_alice29() {
   let raw_file = include_bytes!("../../testdata/alice29");
   let div_input = include_bytes!("../../testdata/alice29-priors.ir");
   let div_raw = divans_decompress_internal(&*div_input).unwrap();
   assert_eq!(raw_file.len(), div_raw.len());
   assert_eq!(&raw_file[..], &div_raw[..]);
}
#[test]
fn test_asyoulik() {
   let raw_file = include_bytes!("../../testdata/asyoulik");
   let div_input = include_bytes!("../../testdata/asyoulik.ir");
   assert_eq!(div_input.len(), 541890);
   let div_raw = divans_decompress_internal(&*div_input).unwrap();
   assert_eq!(raw_file.len(), div_raw.len());
   assert_eq!(&raw_file[..], &div_raw[..]);
}


fn e2e_no_ir(buffer_size: usize, use_serialized_priors: bool, use_brotli: bool, data: &[u8],
             ratio: f64) {
    let mut in_buffer = UnlimitedBuffer::new(data);
    let mut dv_buffer = UnlimitedBuffer::new(&[]);
    let mut rt_buffer = UnlimitedBuffer::new(&[]);
    super::compress_raw(&mut in_buffer,
                        &mut dv_buffer,
                        DivansCompressorOptions{
                            brotli_literal_byte_score: Some(340),
                            use_brotli:BrotliCompressionSetting::UseBrotliCommandSelection,
                            dynamic_context_mixing: Some(if use_brotli {1} else {0}),
                            literal_adaptation: Some([Speed::MED, Speed::MED, Speed::GLACIAL, Speed::GLACIAL]),
                            force_literal_context_mode:None,
                            use_context_map: use_serialized_priors,
                            force_stride_value: StrideSelection::UseBrotliRec, // force stride
                            prior_depth:Some(0),
                            quality:Some(10u16), // quality
                            q9_5:true,
                            window_size:Some(16i32), // window size
                            lgblock:Some(18u32), //lgblock
                            speed_detection_quality: None,
                            prior_bitmask_detection: 1,
                            stride_detection_quality: None,
                            divans_ir_optimizer:1,
                        },
                        buffer_size,
                        use_brotli,
                        true,
                        true).unwrap();
    super::decompress(&mut dv_buffer, &mut rt_buffer, buffer_size, &mut[], false, MULTI).unwrap();
    assert_eq!(rt_buffer.data, in_buffer.data);
    if ratio != 0.0 {
        let actual_ratio =  dv_buffer.data.len() as f64 / in_buffer.data.len() as f64;
        if !(actual_ratio <= ratio) {
            println!("Failed: actual buffer length {} dv_buffer size: {}", in_buffer.data.len(), dv_buffer.data.len());
        }
        assert!(actual_ratio <= ratio);
    }
}

#[test]
fn test_e2e_ones_tinybuf() {
    let data = [1u8, 2u8, 3u8, 4u8,255u8,1u8,2u8,3u8,0u8,1u8,2u8,3u8,8u8,4u8,3u8,
                7u8, 2u8, 3u8, 4u8,6u8,1u8,2u8,4u8,0u8,1u8,16u8,31u8,83u8,43u8,34u8,
                217u8, 252u8, 253u8, 254u8,244u8,251u8,252u8,254u8,250u8,251u8,216u8,231u8,183u8,243u8,234u8,
                217u8, 252u8, 253u8, 254u8,244u8,251u8,252u8,254u8,250u8,251u8,216u8,231u8,183u8,243u8,234u8,
                217u8, 252u8, 253u8, 254u8,244u8,251u8,252u8,254u8,250u8,251u8,216u8,231u8,183u8,243u8,234u8,
                227u8, 252u8, 253u8, 254u8,244u8,251u8,252u8,254u8,250u8,251u8,216u8,231u8,183u8,243u8,234u8,
                237u8, 252u8, 253u8, 254u8,244u8,251u8,252u8,254u8,250u8,251u8,216u8,231u8,183u8,243u8,234u8,
                247u8, 252u8, 253u8, 254u8,244u8,251u8,252u8,254u8,250u8,251u8,216u8,231u8,183u8,243u8,234u8,
                247u8, 252u8, 253u8, 254u8,244u8,251u8,252u8,254u8,250u8,251u8,216u8,231u8,183u8,243u8,234u8,
                247u8, 252u8, 253u8, 254u8,244u8,
    ];
    e2e_no_ir(1, false, false, &data[..], 0.99);
}
#[test]
fn test_e2e_empty_tinybuf() {
    let data = [];
    e2e_no_ir(1, false, false, &data[..], 0.0);
}
#[test]
fn test_e2e_empty() {
    let data = [];
    e2e_no_ir(65536, false, false, &data[..], 0.0);
}
#[test]
fn test_e2e_empty_br_tinybuf() {
    let data = [];
    e2e_no_ir(1, false, true, &data[..], 0.0);
}
#[test]
fn test_e2e_empty_br() {
    let data = [];
    e2e_no_ir(65536, false, true, &data[..], 0.0);
}
#[test]
fn test_e2e_empty_just_flush() {
    use super::ItemVecAllocator;
    use brotli;
    use divans::{DivansCompressorFactory, Compressor};
    use divans;
    let m8 = ItemVecAllocator::<u8>::default();
    let mut obuffer=  [0u8;64];
    let opts = divans::DivansCompressorOptions::default();
    let mut state =super::BrotliFactory::new(
        m8,
        ItemVecAllocator::<u32>::default(),
        ItemVecAllocator::<divans::DefaultCDF16>::default(),
        opts,
        (ItemVecAllocator::<u8>::default(),
         ItemVecAllocator::<u16>::default(),
         ItemVecAllocator::<i32>::default(),
         ItemVecAllocator::<brotli::enc::command::Command>::default(),
         ItemVecAllocator::<u64>::default(),
         ItemVecAllocator::<brotli::enc::util::floatX>::default(),
         ItemVecAllocator::<brotli::enc::vectorization::Mem256f>::default(),
         ItemVecAllocator::<brotli::enc::histogram::HistogramLiteral>::default(),
         ItemVecAllocator::<brotli::enc::histogram::HistogramCommand>::default(),
         ItemVecAllocator::<brotli::enc::histogram::HistogramDistance>::default(),
         ItemVecAllocator::<brotli::enc::cluster::HistogramPair>::default(),
         ItemVecAllocator::<brotli::enc::histogram::ContextType>::default(),
         ItemVecAllocator::<brotli::enc::entropy_encode::HuffmanTree>::default(),
         ItemVecAllocator::<brotli::enc::ZopfliNode>::default(),
         ItemVecAllocator::<brotli::enc::PDF>::default(),
         ItemVecAllocator::<brotli::enc::StaticCommand>::default(),
        ), 
    );
    let mut olim = 0usize;
    match state.flush(&mut obuffer[..],
                      &mut olim) {
        divans::DivansOutputResult::Success => {},
        need => panic!(need),
    }
    let mut rt_buffer = UnlimitedBuffer::new(&[]);
    let mut dv_buffer = UnlimitedBuffer::new(obuffer.split_at(olim).0);
    super::decompress(&mut dv_buffer, &mut rt_buffer, 0, &mut[], false, MULTI).unwrap();
    assert_eq!(rt_buffer.data, &[]);
    state.free();
}
fn e2e_alice(buffer_size: usize, use_serialized_priors: bool) {
   let raw_text_slice = include_bytes!("../../testdata/alice29");
   let raw_text_buffer = UnlimitedBuffer::new(&raw_text_slice[..]);
   e2e_no_ir(buffer_size, use_serialized_priors, true, &raw_text_buffer.data[..], 0.34);
   e2e_no_ir(buffer_size, use_serialized_priors, false, &raw_text_buffer.data[..], 0.46);
   let ir_buffer = if use_serialized_priors {
       UnlimitedBuffer::new(include_bytes!("../../testdata/alice29-priors.ir"))
   } else {
       UnlimitedBuffer::new(include_bytes!("../../testdata/alice29.ir"))
   };
   let mut dv_buffer = UnlimitedBuffer::new(&[]);
   let mut buf_ir = BufReader::new(ir_buffer);
   let mut rt_buffer = UnlimitedBuffer::new(&[]);
   let mut opts = DivansCompressorOptions::default();
   opts.literal_adaptation = Some([Speed::GLACIAL,Speed::MUD,Speed::GLACIAL,Speed::FAST]);
   opts.prior_bitmask_detection=1;
   opts.dynamic_context_mixing=Some(1);
   opts.use_context_map = true;
    super::compress_ir(&mut buf_ir, &mut dv_buffer, opts).unwrap();
    
   super::decompress(&mut dv_buffer, &mut rt_buffer, buffer_size, &mut[], false, MULTI).unwrap();
   println!("dv_buffer size: {}", dv_buffer.data.len());
   let a =  rt_buffer.data;
   let b = raw_text_buffer.data;
   assert_eq!(a, b);
}

#[test]
fn test_e2e_alice() {
    e2e_alice(65536, true);
}

#[test]
fn test_e2e_smallbuf_without_priors() {
    e2e_alice(15, false);
}


#[test]
fn test_e2e_tinybuf() {
    e2e_alice(1, true);
}


#[test]
fn test_e2e_32xx() {
   let raw_text_buffer = UnlimitedBuffer::new(b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
   let ir_buffer = UnlimitedBuffer::new(b"window 22 len 64\ninsert 1 58\ncopy 63 from 1 ctx 3\n");
   let mut dv_buffer = UnlimitedBuffer::new(&[]);
   let mut buf_ir = BufReader::new(ir_buffer);
   let mut rt_buffer = UnlimitedBuffer::new(&[]);
   let mut opts = DivansCompressorOptions::default();
   opts.literal_adaptation = None;

   super::compress_ir(&mut buf_ir, &mut dv_buffer, opts).unwrap();
   super::decompress(&mut dv_buffer, &mut rt_buffer, 15, &mut [],  false, MULTI).unwrap();
   let a =  rt_buffer.data;
   let b = raw_text_buffer.data;
   assert_eq!(a, b);
}


#[test]
fn test_e2e_262145_at() {
   let sbuf = ['@' as u8; 262145];
   let raw_text_buffer = UnlimitedBuffer::new(&sbuf[..]);
   let ir_buffer = UnlimitedBuffer::new(b"window 22 len 262145\ninsert 1 40\ncopy 262144 from 1 ctx 3\n");
   let mut dv_buffer = UnlimitedBuffer::new(&[]);
   let mut buf_ir = BufReader::new(ir_buffer);
   let mut rt_buffer = UnlimitedBuffer::new(&[]);
   let mut opts = DivansCompressorOptions::default();
   opts.literal_adaptation = Some([Speed::MUD, Speed::ROCKET, Speed::FAST, Speed::GLACIAL]);
   opts.use_context_map = true;
   opts.dynamic_context_mixing = Some(2);
   super::compress_ir(&mut buf_ir, &mut dv_buffer, opts).unwrap();
   super::decompress(&mut dv_buffer, &mut rt_buffer, 15, &mut[], false, MULTI).unwrap();
   let a =  rt_buffer.data;
   let b = raw_text_buffer.data;
   assert_eq!(a, b);
}
#[cfg(not(feature="external-literal-probability"))]
const EXTERNAL_PROB_FEATURE:bool = false;
#[cfg(feature="external-literal-probability")]
const EXTERNAL_PROB_FEATURE:bool = true;

#[test]
fn test_e2e_64xp() {
   //let raw_text_buffer = UnlimitedBuffer::new(b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
   let ir_buffer = UnlimitedBuffer::new(b"window 22 len 64\ninsert 64 58585858585858585858585858585858585858585858585858585858585858585858585858585858585858585858585858585858585858585858585858585858 01fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe01010101fe01fefe010101\n");
   let mut dv_buffer = UnlimitedBuffer::new(&[]);
   let mut buf_ir = BufReader::new(ir_buffer);
   //let mut rt_buffer = UnlimitedBuffer::new(&[]);
   let mut opts = DivansCompressorOptions::default();
   opts.literal_adaptation = Some([Speed::FAST, Speed::SLOW, Speed::FAST, Speed::FAST]);
   opts.use_context_map = true;
   opts.dynamic_context_mixing = Some(1);
   match super::compress_ir(&mut buf_ir, &mut dv_buffer, opts) {
      Ok(_) => assert_eq!(EXTERNAL_PROB_FEATURE, true),
      Err(_) => assert_eq!(EXTERNAL_PROB_FEATURE, false),
   };
   //super::decompress(&mut dv_buffer, &mut rt_buffer, 15).unwrap();
   //let a =  rt_buffer.data;
   //let b = raw_text_buffer.data;
   //assert_eq!(a, b);
}


