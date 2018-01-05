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
                BrotliResult::ResultSuccess => return Ok((buf.len())),
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
                                                         HeapAlloc<brotli::enc::entropy_encode::HuffmanTree>>;
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
                                           opts.window_size.unwrap_or(21) as usize,
                                           opts.dynamic_context_mixing.unwrap_or(0),
                                           opts.literal_adaptation,
                                           opts.use_context_map,
                                           opts.force_stride_value,
                                           (
                                               HeapAlloc::<u8>::new(0),
                                               HeapAlloc::<u16>::new(0),
                                               HeapAlloc::<i32>::new(0),
                                               HeapAlloc::<brotli::enc::command::Command>::new(brotli::enc::command::Command::default()),
                                               HeapAlloc::<brotli::enc::util::floatX>::new(0.0 as brotli::enc::util::floatX),
                                               HeapAlloc::<brotli::enc::vectorization::Mem256f>::new(brotli::enc::vectorization::Mem256f::default()),
                                               HeapAlloc::<brotli::enc::histogram::HistogramLiteral>::new(brotli::enc::histogram::HistogramLiteral::default()),
                                               HeapAlloc::<brotli::enc::histogram::HistogramCommand>::new(brotli::enc::histogram::HistogramCommand::default()),
                                               HeapAlloc::<brotli::enc::histogram::HistogramDistance>::new(brotli::enc::histogram::HistogramDistance::default()),
                                               HeapAlloc::<brotli::enc::cluster::HistogramPair>::new(brotli::enc::cluster::HistogramPair::default()),
                                               HeapAlloc::<brotli::enc::histogram::ContextType>::new(brotli::enc::histogram::ContextType::default()),
                                               HeapAlloc::<brotli::enc::entropy_encode::HuffmanTree>::new(brotli::enc::entropy_encode::HuffmanTree::default()),
                                               opts.quality,
                                               opts.lgblock
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
                                           opts.window_size.unwrap_or(21) as usize,
                                           opts.dynamic_context_mixing.unwrap_or(0),
                                           opts.literal_adaptation,
                                           opts.use_context_map,
                                           opts.force_stride_value,
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

