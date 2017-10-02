#![cfg(test)]
extern crate core;
use std::io;

use std::io::Write;
use std::io::BufReader;
use core::cmp;
use super::brotli_decompressor;
use super::brotli_decompressor::BrotliResult;
use super::brotli_decompressor::BrotliDecompressStream;
use super::brotli_decompressor::BrotliState;
use super::brotli_decompressor::HuffmanCode;
use super::util::HeapAllocator;
use super::alloc::{Allocator, SliceWrapperMut, SliceWrapper};
use divans::Speed;
struct UnlimitedBuffer {
  data: Vec<u8>,
  read_offset: usize,
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



fn e2e_alice(buffer_size: usize, use_serialized_priors: bool) {
   let raw_text_as_br = include_bytes!("../../testdata/alice29.br");
   let mut raw_text_buffer = UnlimitedBuffer::new(&[]);
   let mut raw_text_as_br_buffer = UnlimitedBuffer::new(raw_text_as_br);
   brotli_decompressor::BrotliDecompress(&mut raw_text_as_br_buffer,
        &mut raw_text_buffer).unwrap();
   let mut ir_as_br_buffer = if use_serialized_priors {
       UnlimitedBuffer::new(include_bytes!("../../testdata/alice29-priors.ir.br"))
   } else {
       UnlimitedBuffer::new(include_bytes!("../../testdata/alice29.ir.br"))
   };
   let mut ir_buffer = UnlimitedBuffer::new(&[]);
   brotli_decompressor::BrotliDecompress(&mut ir_as_br_buffer,
        &mut ir_buffer).unwrap();
   let mut dv_buffer = UnlimitedBuffer::new(&[]);
   let mut buf_ir = BufReader::new(ir_buffer);
   let mut rt_buffer = UnlimitedBuffer::new(&[]);
   super::compress(&mut buf_ir, &mut dv_buffer, Some(Speed::MUD), true, true, None).unwrap();
   super::decompress(&mut dv_buffer, &mut rt_buffer, buffer_size).unwrap();
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
fn test_e2e_64x() {
   let raw_text_buffer = UnlimitedBuffer::new(b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
   let ir_buffer = UnlimitedBuffer::new(b"window 22 len 64\ninsert 1 58\ncopy 63 from 1 ctx 3\n");
   let mut dv_buffer = UnlimitedBuffer::new(&[]);
   let mut buf_ir = BufReader::new(ir_buffer);
   let mut rt_buffer = UnlimitedBuffer::new(&[]);
   super::compress(&mut buf_ir, &mut dv_buffer, None, true, true, None).unwrap();
   super::decompress(&mut dv_buffer, &mut rt_buffer, 15).unwrap();
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
   super::compress(&mut buf_ir, &mut dv_buffer, Some(Speed::ROCKET), true, true, None).unwrap();
   super::decompress(&mut dv_buffer, &mut rt_buffer, 15).unwrap();
   let a =  rt_buffer.data;
   let b = raw_text_buffer.data;
   assert_eq!(a, b);
}
