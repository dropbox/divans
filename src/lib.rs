#![no_std]
#[cfg(test)]
#[macro_use]
extern crate std;
extern crate alloc_no_stdlib as alloc;
extern crate brotli_decompressor;
mod interface;
mod probability;
mod debug_encoder;
mod encoder;
mod cmd_to_raw;
mod codec;
mod cmd_to_divans;
mod divans_to_raw;
pub use brotli_decompressor::{BrotliResult};
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use interface::{Command, Decompressor, Compressor, LiteralCommand, CopyCommand, DictCommand};
pub use cmd_to_raw::DivansRecodeState;
pub use codec::CMD_BUFFER_SIZE;
pub use divans_to_raw::DecoderSpecialization;
pub use cmd_to_divans::EncoderSpecialization;
pub use codec::{EncoderOrDecoderSpecialization, DivansCodec};
/*
pub struct DivansDecompressor<DivansDecoder:Decoder, RawRecoder: Recoder> {
    decoder: DivansDecoder,
    recoder: RawRecoder,
    buffer: [Command<DivansDecoder::CommandSliceType>; CMD_BUFFER_SIZE],
    buffer_size: usize,
    buffer_offset: usize,
    decode_complete: bool,
}
impl<DivansDecoder:Decoder, RawRecoder: Recoder> DivansDecompressor<DivansDecoder, RawRecoder> {
    pub fn new(decoder: DivansDecoder,
               recoder: RawRecoder) -> Self{

        DivansDecompressor {
            decoder:decoder,
            recoder:recoder,
            buffer:[Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
                    Command::<DivansDecoder::CommandSliceType>::nop(),
            ],
            buffer_size: 0,
            buffer_offset: 0,
            decode_complete: false,
        }
    }
    pub fn decode(&mut self,
                  input:&[u8],
                  input_offset: &mut usize,
                  output: &mut [u8],
                  output_offset: &mut usize) -> BrotliResult {
        let input_len = input.len();
        let output_len = output.len();
        let mut needs_input = false;
        loop {
            if self.buffer_size == self.buffer_offset {
                self.buffer_size = 0;
                self.buffer_offset = 0;
            }
            if *input_offset < input_len && self.buffer_size < self.buffer.len() && self.decode_complete == false && needs_input == false{
                match self.decoder.decode(input,
                                     input_offset,
                                     &mut self.buffer,
                                     &mut self.buffer_size) {
                    BrotliResult::NeedsMoreInput => {
                        needs_input = true;
                    },
                    BrotliResult::NeedsMoreOutput => {
                    },
                    BrotliResult::ResultFailure => {
                        return BrotliResult::ResultFailure;
                    },
                    BrotliResult::ResultSuccess => {
                        self.decode_complete = true;
                    }
                }
            }
            if self.buffer_offset < self.buffer_size && *output_offset < output_len {
                match self.recoder.recode(self.buffer.split_at(self.buffer_size).0,
                                     &mut self.buffer_offset,
                                     output,
                                     output_offset) {
                    BrotliResult::NeedsMoreInput => {
                        assert_eq!(self.buffer_size, self.buffer_offset);
                        if needs_input {
                            return BrotliResult::NeedsMoreInput;
                        }
                        if self.decode_complete {
                            return BrotliResult::ResultSuccess;
                        }
                    },
                    BrotliResult::NeedsMoreOutput => {
                        return BrotliResult::NeedsMoreOutput;
                    },
                    BrotliResult::ResultFailure => {
                        return BrotliResult::ResultFailure;
                    },
                    BrotliResult::ResultSuccess => {
                        if self.decode_complete {
                            return BrotliResult::ResultSuccess;
                        }
                    },
                }
            }
        }
    }
}


*/
