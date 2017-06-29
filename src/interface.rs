use alloc::{SliceWrapper};
use brotli_decompressor::BrotliResult;
use super::probability::{CDFUpdater, CDF16};


#[derive(Debug)]
pub struct CopyCommand {
    pub distance: u32,
    pub num_bytes: u32,
}

impl CopyCommand {
    pub fn nop() -> Self{
        CopyCommand{distance:1, num_bytes:0}
    }
}

#[derive(Debug)]
pub struct DictCommand {
    pub word_size: u8,
    pub transform: u8,
    pub final_size: u8,
    pub empty: u8,
    pub word_id: u32,
}
impl DictCommand {
    pub fn nop() -> Self{
        DictCommand{word_size:0, transform:0, final_size:0, empty:1, word_id:0}
    }
}


#[derive(Debug)]
pub struct LiteralCommand<SliceType:SliceWrapper<u8>> {
    pub data: SliceType,
}

impl<SliceType:SliceWrapper<u8>+Default> LiteralCommand<SliceType> {
    pub fn nop() -> Self {
        LiteralCommand{data:SliceType::default()}
    }
}


#[derive(Debug)]
pub enum Command<SliceType:SliceWrapper<u8> > {
    Copy(CopyCommand),
    Dict(DictCommand),
    Literal(LiteralCommand<SliceType>),
}

impl<SliceType:SliceWrapper<u8>> Default for Command<SliceType> {
    fn default() -> Self {
        Command::<SliceType>::nop()
    }
}

impl<SliceType:SliceWrapper<u8>> Command<SliceType> {
    pub fn nop() -> Command<SliceType> {
        Command::Copy(CopyCommand::nop())
    }
}

pub trait Compressor {
    fn encode<SliceType:SliceWrapper<u8>+Default>(&mut self,
                                          input:&[Command<SliceType>],
                                          input_offset : &mut usize,
                                          output :&mut[u8],
                                          output_offset: &mut usize) -> BrotliResult;
    fn flush(&mut self,
                                          output :&mut[u8],
                                          output_offset: &mut usize) -> BrotliResult;
}

pub trait Decompressor {
    fn decode(&mut self,
              input:&[u8],
              input_offset : &mut usize,
              output :&mut[u8],
              output_offset: &mut usize) -> BrotliResult;
}

pub trait CommandDecoder {
    type CommandSliceType: SliceWrapper<u8>;
    fn decode(
        &mut self,
        input: &[u8],
        input_offset: &mut usize,
        output: &mut [Command<Self::CommandSliceType>],
        output_offset: &mut usize) -> BrotliResult;
    fn flush(&mut self) -> BrotliResult;
}


pub trait ArithmeticEncoderOrDecoder {
    // note: only one of these buffers must be nonzero,
    // depending on if it is in encode or decode mode
    fn drain_or_fill_internal_buffer(&mut self,
                                     input_buffer:&[u8],
                                     input_offset:&mut usize,
                                     output_buffer:&mut [u8],
                                     output_offset: &mut usize) -> BrotliResult;
    fn get_or_put_bit(&mut self,
                      bit: &mut bool,
                      prob_of_false: u8);
    fn get_or_put_nibble<U:CDFUpdater> (&mut self,
                                        nibble: &mut u8,
                                        prob: &CDF16<U>);
    fn close(&mut self) -> BrotliResult;
}
