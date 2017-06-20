use alloc::{SliceWrapper};
use brotli_decompressor::BrotliResult;
use super::probability::{CDFUpdater, CDF16};
#[derive(Debug)]
pub struct CopyCommand {
    pub distance: u32,
    pub num_bytes: u32,
}

#[derive(Debug)]
pub struct DictCommand {
    pub word_size: u8,
    pub transform: u8,
    pub final_size: u8,
    pub _empty: u8,
    pub word_id: u32,
}

#[derive(Debug)]
pub struct LiteralCommand<SliceType:SliceWrapper<u8>> {
    pub data: SliceType,
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
        Command::Copy(CopyCommand{distance:1, num_bytes:0})
    }
}

pub trait Recoder {
    fn recode<SliceType:SliceWrapper<u8>>(&mut self,
                  input:&[Command<SliceType>],
                  input_offset : &mut usize,
                  output :&mut[u8],
                  output_offset: &mut usize) -> BrotliResult;
}

pub trait Decoder {
    type CommandSliceType: SliceWrapper<u8>;
    fn decode(
        &mut self,
        input: &[u8],
        input_offset: &mut usize,
        output: &mut [Command<Self::CommandSliceType>],
        output_offset: &mut usize) -> BrotliResult;
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
    fn flush(&mut self);
}
