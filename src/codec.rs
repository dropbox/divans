use alloc::{SliceWrapper, Allocator};
use brotli_decompressor::BrotliResult;
pub const CMD_BUFFER_SIZE: usize = 16;

use super::interface::{
    CopyCommand,
    DictCommand,
    LiteralCommand,
    Command,
    Decoder,
    Recoder,
    ArithmeticEncoderOrDecoder
};

pub struct DivansCodec<ArithmeticCoder:ArithmeticEncoderOrDecoder,
                   AllocU8: Allocator<u8>> {
    coder: ArithmeticCoder,
    m8: AllocU8,
    // this holds recent Command::LiteralCommand's buffers when
    // those commands are repurposed for other things like LiteralCommand
    literal_cache: [AllocatedMemory; CMD_BUFFER_SIZE],
    // need state variable describing the item we are building
}

impl<ArithmeticCoder:ArithmeticEncoderOrDecoder,
     AllocU8: Allocator<u8>> DivansCodec<ArithmeticCoder, AllocU8> {
    pub fn encode_or_decode<ISl:SliceWrapper<u8>>(input_bytes: &[u8],
                                                  input_bytes_offset: &mut usize,
                                                  output_bytes: &mut [u8],
                                                  output_bytes_offset: &mut usize,
                                                  input_commands: &[Command<ISl>],
                                                  input_command_offset: &mut usize,
                                                  output_commands: &mut[Command<AllocU8::AllocatedMemory>],
                                                  output_command_offset: &mut usize) -> BrotliResult {
        BrotliResult::ResultFailure
    }
                        
}
