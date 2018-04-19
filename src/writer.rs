#![cfg(not(feature="no-stdlib"))]
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use alloc::HeapAlloc;
use std::io;
use std::io::Write;
use super::BrotliResult;
use ::interface::{Compressor, DivansCompressorFactory, Decompressor};
use ::DivansDecompressorFactory;
use ::brotli;
use ::interface;

trait Processor {
   fn process(&mut self, input:&[u8], input_offset:&mut usize, output:&mut [u8], output_offset:&mut usize) -> BrotliResult;
   fn close(&mut self, output:&mut [u8], output_offset:&mut usize) -> BrotliResult;
}

struct GenWriter<W: Write,
                 P:Processor,
                 BufferType:SliceWrapperMut<u8>> {
  compressor: P,
  output_buffer: BufferType,
  has_flushed: bool,
  output: W,
}


impl<W:Write, P:Processor, BufferType:SliceWrapperMut<u8>> Write for GenWriter<W,P,BufferType> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        let mut avail_in = buf.len();
        let mut input_offset : usize = 0;
        loop {
            let mut output_offset = 0;
            let old_input_offset = input_offset;
            let op_result = self.compressor.process(
                &buf.split_at(avail_in + input_offset).0,
                &mut input_offset,
                self.output_buffer.slice_mut(),
                &mut output_offset);
            avail_in -= input_offset - old_input_offset;
            match self.output.write_all(&self.output_buffer.slice_mut()[..output_offset]) {
                Ok(_) => {},
                Err(e) => return Err(e),
            }
            match op_result {
                BrotliResult::NeedsMoreInput => assert_eq!(avail_in, 0),
                BrotliResult::NeedsMoreOutput => continue,
                BrotliResult::ResultSuccess => return Ok(buf.len()),
                BrotliResult::ResultFailure => return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid input")),
            }
            if avail_in == 0 {
                break
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), io::Error> {
        while !self.has_flushed {
            let mut output_offset = 0;
            let ret = self.compressor.close(
                self.output_buffer.slice_mut(),
                &mut output_offset);
            match self.output.write_all(&self.output_buffer.slice_mut()[..output_offset]) {
                Ok(_) => {},
                Err(e) => return Err(e),
            }
            match ret {
                BrotliResult::NeedsMoreInput | BrotliResult::ResultFailure => {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid input"))
                }
                BrotliResult::NeedsMoreOutput => {},
                BrotliResult::ResultSuccess => {
                    self.has_flushed = true;
                }
            }
        }
        self.output.flush()
    }
}
impl<W:Write, C:Processor, BufferType:SliceWrapperMut<u8>> GenWriter<W,C,BufferType>{
    pub fn new(writer:W, compressor:C, buffer:BufferType, needs_flush: bool) ->Self {
        GenWriter {
            output:writer,
            compressor:compressor,
            output_buffer: buffer,
            has_flushed: !needs_flush,
        }
    }
}
type DivansBrotliFactory = ::BrotliDivansHybridCompressorFactory<HeapAlloc<u8>,
                                                         HeapAlloc<u16>,
                                                         HeapAlloc<u32>,
                                                         HeapAlloc<i32>,
                                                         HeapAlloc<u64>,
                                                         HeapAlloc<brotli::enc::command::Command>,
                                                         HeapAlloc<::CDF2>,
                                                         HeapAlloc<::DefaultCDF16>,
                                                         HeapAlloc<brotli::enc::util::floatX>,
                                                         HeapAlloc<brotli::enc::vectorization::Mem256f>,
                                                         HeapAlloc<brotli::enc::histogram::HistogramLiteral>,
                                                         HeapAlloc<brotli::enc::histogram::HistogramCommand>,
                                                         HeapAlloc<brotli::enc::histogram::HistogramDistance>,
                                                         HeapAlloc<brotli::enc::cluster::HistogramPair>,
                                                         HeapAlloc<brotli::enc::histogram::ContextType>,
                                                         HeapAlloc<brotli::enc::entropy_encode::HuffmanTree>,
                                                         HeapAlloc<brotli::enc::ZopfliNode>>;
type DivansBrotliConstructedCompressor = <DivansBrotliFactory as ::DivansCompressorFactory<HeapAlloc<u8>,
                                                                                           HeapAlloc<u32>,
                                                                                           HeapAlloc<::CDF2>,
                                                                                           HeapAlloc<::DefaultCDF16>>>::ConstructedCompressor;
impl<T:Compressor> Processor for T {
   fn process(&mut self, input:&[u8], input_offset:&mut usize, output:&mut [u8], output_offset:&mut usize) -> BrotliResult {
       self.encode(input, input_offset, output, output_offset)
   }
   fn close(&mut self, output:&mut [u8], output_offset:&mut usize) -> BrotliResult{
      self.flush(output, output_offset)
   }

}
pub struct DivansBrotliHybridCompressorWriter<W:Write>(GenWriter<W,
                                                                DivansBrotliConstructedCompressor,
                                                                <HeapAlloc<u8> as Allocator<u8>>::AllocatedMemory,
                                                               >);
impl<W:Write> Write for DivansBrotliHybridCompressorWriter<W> {
	fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.0.write(buf)
    }
	fn flush(&mut self) -> Result<(), io::Error> {
        self.0.flush()
    }
}
impl<W:Write> DivansBrotliHybridCompressorWriter<W> {
    pub fn new(writer: W, opts: interface::DivansCompressorOptions, mut buffer_size: usize) -> Self {
       if buffer_size == 0 {
          buffer_size = 4096;
       }
       let mut m8 = HeapAlloc::<u8>::new(0);
       let buffer = m8.alloc_cell(buffer_size);
       DivansBrotliHybridCompressorWriter::<W>(
           GenWriter::<W,
                       DivansBrotliConstructedCompressor,
                       <HeapAlloc<u8> as Allocator<u8>>::AllocatedMemory>::new(
                          writer,
                          DivansBrotliFactory::new(
                                           m8,
                                           HeapAlloc::<u32>::new(0),
                                           HeapAlloc::<::CDF2>::new(::CDF2::default()),
                                           HeapAlloc::<::DefaultCDF16>::new(::DefaultCDF16::default()),
                                           opts,
                                           (
                                               HeapAlloc::<u8>::new(0),
                                               HeapAlloc::<u16>::new(0),
                                               HeapAlloc::<i32>::new(0),
                                               HeapAlloc::<brotli::enc::command::Command>::new(brotli::enc::command::Command::default()),
                                               HeapAlloc::<u64>::new(0),
                                               HeapAlloc::<brotli::enc::util::floatX>::new(0.0 as brotli::enc::util::floatX),
                                               HeapAlloc::<brotli::enc::vectorization::Mem256f>::new(brotli::enc::vectorization::Mem256f::default()),
                                               HeapAlloc::<brotli::enc::histogram::HistogramLiteral>::new(brotli::enc::histogram::HistogramLiteral::default()),
                                               HeapAlloc::<brotli::enc::histogram::HistogramCommand>::new(brotli::enc::histogram::HistogramCommand::default()),
                                               HeapAlloc::<brotli::enc::histogram::HistogramDistance>::new(brotli::enc::histogram::HistogramDistance::default()),
                                               HeapAlloc::<brotli::enc::cluster::HistogramPair>::new(brotli::enc::cluster::HistogramPair::default()),
                                               HeapAlloc::<brotli::enc::histogram::ContextType>::new(brotli::enc::histogram::ContextType::default()),
                                               HeapAlloc::<brotli::enc::entropy_encode::HuffmanTree>::new(brotli::enc::entropy_encode::HuffmanTree::default()),
                                               HeapAlloc::<brotli::enc::ZopfliNode>::new(brotli::enc::ZopfliNode::default()),
                                           )),
                          buffer,
                          true,
                       ))
    }
}


type DivansCustomFactory = ::DivansCompressorFactoryStruct<HeapAlloc<u8>,
                                                         HeapAlloc<::CDF2>,
                                                         HeapAlloc<::DefaultCDF16>>;
type DivansCustomConstructedCompressor = <DivansCustomFactory as ::DivansCompressorFactory<HeapAlloc<u8>,
                                                                                           HeapAlloc<u32>,
                                                                                           HeapAlloc<::CDF2>,
                                                                                           HeapAlloc<::DefaultCDF16>>>::ConstructedCompressor;
pub struct DivansExperimentalCompressorWriter<W:Write>(GenWriter<W,
                                                                DivansCustomConstructedCompressor,
                                                                 <HeapAlloc<u8> as Allocator<u8>>::AllocatedMemory,
                                                               >);
impl<W:Write> Write for DivansExperimentalCompressorWriter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.0.write(buf)
    }
	fn flush(&mut self) -> Result<(), io::Error> {
        self.0.flush()
    }
}
impl<W:Write> DivansExperimentalCompressorWriter<W> {
    pub fn new(writer: W, opts: interface::DivansCompressorOptions, mut buffer_size: usize) -> Self {
       if buffer_size == 0 {
          buffer_size = 4096;
       }
       let mut m8 = HeapAlloc::<u8>::new(0);
       let buffer = m8.alloc_cell(buffer_size);
       DivansExperimentalCompressorWriter::<W>(
           GenWriter::<W,
                       DivansCustomConstructedCompressor,
                       <HeapAlloc<u8> as Allocator<u8>>::AllocatedMemory>::new(
                          writer,
                          DivansCustomFactory::new(
                                           m8,
                                           HeapAlloc::<u32>::new(0),
                                           HeapAlloc::<::CDF2>::new(::CDF2::default()),
                                           HeapAlloc::<::DefaultCDF16>::new(::DefaultCDF16::default()),
                                           opts,
                                           ()),
                          buffer,
                          true,
                       ))
    }
}


type StandardDivansDecompressorFactory = ::DivansDecompressorFactoryStruct<HeapAlloc<u8>,
                                                                     HeapAlloc<::CDF2>,
                                                                     HeapAlloc<::DefaultCDF16>>;
type DivansConstructedDecompressor = ::DivansDecompressor<<StandardDivansDecompressorFactory as ::DivansDecompressorFactory<HeapAlloc<u8>,
                                                                                                       HeapAlloc<::CDF2>,
                                                                                                                            HeapAlloc<::DefaultCDF16>>
                                                           >::DefaultDecoder,
                                                          HeapAlloc<u8>,
                                                          HeapAlloc<::CDF2>,
                                                          HeapAlloc<::DefaultCDF16>>;
impl Processor for DivansConstructedDecompressor {
   fn process(&mut self, input:&[u8], input_offset:&mut usize, output:&mut [u8], output_offset:&mut usize) -> BrotliResult {
       self.decode(input, input_offset, output, output_offset)
   }
   fn close(&mut self, output:&mut [u8], output_offset:&mut usize) -> BrotliResult{
       let mut input_offset = 0usize;
       self.decode(&[], &mut input_offset, output, output_offset)
   }

}
pub struct DivansDecompressorWriter<W:Write>(GenWriter<W,
                                                      DivansConstructedDecompressor,
                                                      <HeapAlloc<u8> as Allocator<u8>>::AllocatedMemory,
                                                      >);
impl<W:Write> Write for DivansDecompressorWriter<W> {
	fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.0.write(buf)
    }
	fn flush(&mut self) -> Result<(), io::Error> {
        self.0.flush()
    }
}
impl<W:Write> DivansDecompressorWriter<W> {
    pub fn new(writer: W, mut buffer_size: usize) -> Self {
       if buffer_size == 0 {
          buffer_size = 4096;
       }
       let mut m8 = HeapAlloc::<u8>::new(0);
       let buffer = m8.alloc_cell(buffer_size);
       DivansDecompressorWriter::<W>(
           GenWriter::<W,
                       DivansConstructedDecompressor,
                       <HeapAlloc<u8> as Allocator<u8>>::AllocatedMemory>::new(
                          writer,
                          StandardDivansDecompressorFactory::new(
                              m8,
                              HeapAlloc::<::CDF2>::new(::CDF2::default()),
                              HeapAlloc::<::DefaultCDF16>::new(::DefaultCDF16::default()),
                          ),
                          buffer,
                          false,
                       ))
    }
}

#[cfg(test)]
mod test {
    use core;
    use std::vec::Vec;
    use std::io;
    use std::io::Write;
    use ::interface;
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

    impl io::Write for UnlimitedBuffer {
        fn write(self: &mut Self, buf: &[u8]) -> io::Result<usize> {
            self.data.extend(buf);
            return Ok(buf.len());
        }
        fn flush(self: &mut Self) -> io::Result<()> {
            return Ok(());
        }
    }

    struct Tee<'a, W:io::Write> {
        writer: W,
        output: &'a mut UnlimitedBuffer,
    }
    impl<'a, W:Write> io::Write for Tee<'a, W> {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            let ret = self.writer.write(data);
            match ret {
                Err(_) => {},
                Ok(size) => {
                    let xret = self.output.write(&data[..size]);
                    if let Ok(xsize) = xret {
                        assert_eq!(xsize, size); // we know unlimited buffer won't let us down
                    } else {
                        unreachable!();
                    }
                }
            }
            ret
        }
        fn flush(&mut self) -> io::Result<()> {
           self.writer.flush()
        }
    }
    fn hy_writer_tst(data:&[u8], opts: interface::DivansCompressorOptions, buffer_size: usize){
        let mut dest = UnlimitedBuffer::new(&[]);
        let mut ub = UnlimitedBuffer::new(&mut []);
        {
          let tmp = UnlimitedBuffer::new(&[]);
          let dest_tee = Tee::<UnlimitedBuffer> {
              writer: tmp,
              output: &mut dest,
          };
          let decompress = super::DivansDecompressorWriter::new(dest_tee, buffer_size);
            let tee = Tee::<::DivansDecompressorWriter<Tee<UnlimitedBuffer>>> {
                writer:decompress,
                output: &mut ub,
            };
            let mut compress = ::DivansBrotliHybridCompressorWriter::new(tee, opts, buffer_size);
            let mut offset: usize = 0;
            while offset < data.len() {
                match compress.write(&data[offset..core::cmp::min(offset + buffer_size, data.len())]) {
                    Err(e) => panic!(e),
                    Ok(size) => {
                        if size == 0 {
                            break;
                        }
                        offset += size;
                    }
                }
            }
            if let Err(e) = compress.flush() {
                 panic!(e);
            }
        }
        assert_eq!(dest.written(), data);
        assert!(ub.data.len() < data.len());
        print!("Compressed {} to {}...\n", ub.data.len(), data.len());
    }
    fn experimental_writer_tst(data:&[u8], opts: interface::DivansCompressorOptions, buffer_size: usize){
        let mut dest = UnlimitedBuffer::new(&[]);
        let mut ub = UnlimitedBuffer::new(&mut []);
        {
          let tmp = UnlimitedBuffer::new(&[]);
          let dest_tee = Tee::<UnlimitedBuffer> {
              writer: tmp,
              output: &mut dest,
          };
          let decompress = super::DivansDecompressorWriter::new(dest_tee, buffer_size);
            let tee = Tee::<::DivansDecompressorWriter<Tee<UnlimitedBuffer>>> {
                writer:decompress,
                output: &mut ub,
            };
            let mut compress = ::DivansBrotliHybridCompressorWriter::new(tee, opts, buffer_size);
            let mut offset: usize = 0;
            while offset < data.len() {
                match compress.write(&data[offset..core::cmp::min(offset + buffer_size, data.len())]) {
                    Err(e) => panic!(e),
                    Ok(size) => {
                        if size == 0 {
                            break;
                        }
                        offset += size;
                    }
                }
            }
            if let Err(e) = compress.flush() {
                 panic!(e);
            }
        }
        assert_eq!(dest.written(), data);
        assert!(ub.data.len() < data.len());
        print!("Compressed {} to {}...\n", ub.data.len(), data.len());
    }
    #[test]
    fn test_hybrid_writer_compressor_on_alice_small_buffer() {
        hy_writer_tst(include_bytes!("../testdata/alice29"),
                       interface::DivansCompressorOptions{
                           literal_adaptation:None,
                           force_literal_context_mode:None,
                           brotli_literal_byte_score:None,
                           window_size:Some(16),
                           lgblock:Some(16),
                           quality:Some(11),
                           q9_5:true,
                           prior_depth:Some(0),
                           dynamic_context_mixing:None,
                           use_brotli:interface::BrotliCompressionSetting::default(),
                           use_context_map:true,
                           force_stride_value: interface::StrideSelection::default(),
                           speed_detection_quality: None,
                           stride_detection_quality: Some(2),
                           prior_bitmask_detection: 1,
                       },
                       1);
    }
    #[test]
    fn test_hybrid_writer_compressor_on_alice_full() {
        hy_writer_tst(include_bytes!("../testdata/alice29"),
                       interface::DivansCompressorOptions{
                           literal_adaptation:None,
                           force_literal_context_mode:None,
                           brotli_literal_byte_score:None,
                           window_size:Some(22),
                           q9_5:false,
                           lgblock:None,
                           quality:None,
                           dynamic_context_mixing:Some(2),
                           use_brotli:interface::BrotliCompressionSetting::default(),
                           use_context_map:true,
                           prior_depth:Some(1),
                           force_stride_value: interface::StrideSelection::Stride1,
                           speed_detection_quality: None,
                           prior_bitmask_detection: 0,
                           stride_detection_quality: None,
                       },
                       4095);
    }
    #[test]
    fn test_hybrid_writer_compressor_on_unicode_full() {
        hy_writer_tst(include_bytes!("../testdata/random_then_unicode"),
                       interface::DivansCompressorOptions{
                           literal_adaptation:None,
                           force_literal_context_mode:None,
                           brotli_literal_byte_score:None,
                           window_size:Some(22),
                           lgblock:None,
                           quality:Some(8),
                           q9_5:false,
                           prior_depth:None,
                           dynamic_context_mixing:Some(2),
                           use_brotli:interface::BrotliCompressionSetting::default(),
                           use_context_map:true,
                           force_stride_value: interface::StrideSelection::Stride1,
                           speed_detection_quality: None,
                           prior_bitmask_detection: 1,
                           stride_detection_quality: None,
                       },
                       4095);
    }
    #[test]
    fn test_experimental_writer_compressor_on_alice_full() {
        experimental_writer_tst(include_bytes!("../testdata/alice29"),
                       interface::DivansCompressorOptions{
                           literal_adaptation:None,
                           force_literal_context_mode:None,
                           brotli_literal_byte_score:None,
                           window_size:Some(22),
                           lgblock:None,
                           quality:None,
                           q9_5:true,
                           prior_depth:Some(2),
                           dynamic_context_mixing:Some(2),
                           prior_bitmask_detection: 1,
                           use_brotli:interface::BrotliCompressionSetting::default(),
                           use_context_map:true,
                           force_stride_value: interface::StrideSelection::UseBrotliRec,
                           speed_detection_quality: None,
                           stride_detection_quality: Some(1),
                       },
                       3);
    }
}
