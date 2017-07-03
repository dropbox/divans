#![cfg(test)]
use std::vec::Vec;
use alloc;
use core;

use core::cmp;
use std::io;
use std::io::BufReader;
use brotli_decompressor;
struct Buffer {
  data: Vec<u8>,
  read_offset: usize,
}
impl Buffer {
  pub fn new(buf: &[u8]) -> Buffer {
    let mut ret = Buffer {
      data: Vec::<u8>::new(),
      read_offset: 0,
    };
    ret.data.extend(buf);
    return ret;
  }
}
impl io::Read for Buffer {
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
impl io::Write for Buffer {
  fn write(self: &mut Self, buf: &[u8]) -> io::Result<usize> {
    self.data.extend(buf);
    return Ok(buf.len());
  }
  fn flush(self: &mut Self) -> io::Result<()> {
    return Ok(());
  }
}



#[test]
fn test_alice() {
   let raw_text_as_br = include_bytes!("testdata/alice29.txt.br");
   let mut raw_text_buffer = Buffer::new(&[]);
   let mut raw_text_as_br_buffer = Buffer::new(raw_text_as_br);
   brotli_decompressor::BrotliDecompress(&mut raw_text_as_br_buffer,
        &mut raw_text_buffer).unwrap();
   let ir_as_br = include_bytes!("testdata/alice29.ir.br");
   let mut ir_buffer = Buffer::new(&[]);
   let mut ir_as_br_buffer = Buffer::new(ir_as_br);
   brotli_decompressor::BrotliDecompress(&mut ir_as_br_buffer,
        &mut ir_buffer).unwrap();
   let mut dv_buffer = Buffer::new(&[]);
   let mut buf_ir = BufReader::new(ir_buffer);
   let mut rt_buffer = Buffer::new(&[]);
   super::compress(&mut buf_ir, &mut dv_buffer).unwrap();
   super::decompress(&mut dv_buffer, &mut rt_buffer, 65536).unwrap();
   let a =  rt_buffer.data;
   let b = raw_text_buffer.data;
   assert_eq!(a, b);
}
