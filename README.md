% divANS Module

# Overview
The `divANS` crate is meant to be used for generic data compression.
The algorithm has been tuned to significantly favor gains in compression ratio
over performance, operating at line speeds of 150 Mbit/s.

The name originates from "divided-ANS" since the intermediate representation is divided from the ANS codec

More information at <https://blogs.dropbox.com/tech/2018/06/building-better-compression-together-with-divans/>


Divans should primarily be considered for cold storage and compression research.
The compression algorithm is highly modular and new algorithms only need to be
written a single time since generic trait specialization constructs optimized variants of
the codec for both compression and decompression at compile time.


# Rust Usage

## Decompression
```rust
extern crate divans;
fn main() {
    use std::io;
    let stdin = &mut io::stdin();
    {
        use std::io::{Read, Write};
        let mut reader = divans::DivansDecompressorReader::new(
            stdin,
            4096, // buffer size
        );
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf[..]) {
                Err(e) => {
                    if let io::ErrorKind::Interrupted = e.kind() {
                        continue;
                    }
                    panic!(e);
                }
                Ok(size) => {
                    if size == 0 {
                        break;
                    }
                    match io::stdout().write_all(&buf[..size]) {
                        Err(e) => panic!(e),
                        Ok(_) => {},
                    }
                }
            }
        }
    }   
}
```
## Compression

```rust
extern crate divans;
fn main() {
    use std::io;
    let stdout = &mut io::stdout();
    {
        use std::io::{Read, Write};
        let mut writer = divans::DivansBrotliHybridCompressorWriter::new(
            stdout,
            divans::DivansCompressorOptions{
                literal_adaptation:None, // should we override how fast the cdfs converge for literals?
                window_size:Some(22), // log 2 of the window size
                lgblock:None, // should we override how often metablocks are created in brotli
                quality:Some(11), // the quality of brotli commands
                dynamic_context_mixing:Some(2), // if we want to mix together the stride prediction and the context map
                use_brotli:divans::BrotliCompressionSetting::default(), // ignored
                use_context_map:true, // whether we should use the brotli context map in addition to the last 8 bits of each byte as a prior
                force_stride_value: divans::StrideSelection::UseBrotliRec, // if we should use brotli to decide on the stride
            },
            4096, // internal buffer size
        );
        let mut buf = [0u8; 4096];
        loop {
            match io::stdin().read(&mut buf[..]) {
                Err(e) => {
                    if let io::ErrorKind::Interrupted = e.kind() {
                        continue;
                    }
                    panic!(e);
                }
                Ok(size) => {
                    if size == 0 {
                        match writer.flush() {
                            Err(e) => {
                                if let io::ErrorKind::Interrupted = e.kind() {
                                    continue;
                                }
                                panic!(e)
                            }
                            Ok(_) => break,
                        }
                    }
                    match writer.write_all(&buf[..size]) {
                        Err(e) => panic!(e),
                        Ok(_) => {},
                    }
                }
            }
        }
    }
}
```

# C usage
The C api is a standard compression API like the one that zlib provides.
Despite being rust code, no allocations are made unless the CAllocator struct is passed in with
the custom_malloc field set to NULL.
This means that any user of the divans library may provide their own allocation system and
all allocations will go through that allocation system.
The pointers returned by custom_malloc must be 32-byte aligned.

## Compression
```C
#include "divans/ffi.h"
// compress to stdout
DivansResult compress(const unsigned char *data, size_t len) {
    unsigned char buf[4096];
    struct CAllocator alloc = {custom_malloc, custom_free, custom_alloc_opaque}; // set all 3 to NULL to use rust allocators
    struct DivansCompressorState *state = divans_new_compressor_with_custom_alloc(alloc);
    divans_set_option(state, DIVANS_OPTION_USE_CONTEXT_MAP, 1);
    divans_set_option(state, DIVANS_OPTION_DYNAMIC_CONTEXT_MIXING, 2);
    divans_set_option(state, DIVANS_OPTION_QUALITY, 11);
    while (len) {
        size_t read_offset = 0;
        size_t buf_offset = 0;
        DivansResult res = divans_encode(state,
                                         data, len, &read_offset,
                                         buf, sizeof(buf), &buf_offset);
        if (res == DIVANS_FAILURE) {
            divans_free_compressor(state);
            return res;
        }
        data += read_offset;
        len -= read_offset;
        fwrite(buf, buf_offset, 1, stdout);
    }
    DivansResult res;
    do {
        size_t buf_offset = 0;
        res = divans_encode_flush(state,
                                  buf, sizeof(buf), &buf_offset);
        if (res == DIVANS_FAILURE) {
            divans_free_compressor(state);
            return res;
        }
        fwrite(buf, buf_offset, 1, stdout);
    } while(res != DIVANS_SUCCESS);
    divans_free_compressor(state);
    return DIVANS_SUCCESS;
}
```
## Decompression
```C
#include "divans/ffi.h"
//decompress to stdout
DivansResult decompress(const unsigned char *data, size_t len) {
    unsigned char buf[4096];
    struct CAllocator alloc = {custom_malloc, custom_free, custom_alloc_opaque}; // set all 3 to NULL for using rust allocators
    struct DivansDecompressorState *state = divans_new_decompressor_with_custom_alloc(alloc);
    DivansResult res;
    do {
        size_t read_offset = 0;
        size_t buf_offset = 0;
        res = divans_decode(state,
                            data, len, &read_offset,
                            buf, sizeof(buf), &buf_offset);
        if (res == DIVANS_FAILURE || (res == DIVANS_NEEDS_MORE_INPUT && len == 0)) {
            divans_free_decompressor(state);
            return res;
        }
        data += read_offset;
        len -= read_offset;
        fwrite(buf, buf_offset, 1, stdout);
    } while (res != DIVANS_SUCCESS);
    divans_free_decompressor(state);
    return DIVANS_SUCCESS;
}
```

# Structure of the divANS codebase

## Top Level Modules
| Module                | Purpose |
|:------:               |-------|
| probability           | Optimized implementations of 16-wide 4-bit CDF's that support online training and renormalization |
| codec/interface       | CrossCommandState tracks data to be kept between brotli commands. Examples include CDF's, the previous few bytes, the ring buffer for copies, etc |
| codec/dict            | Encode/decode parts of the file that may arise from the included brotli dictionary |
| codec/copy            | Encode/decode parts of the file that have already been seen before and are still in the ring buffer |
| codec/block_type      | Encode/decode markers in the file which divans can use as a prior for literals, distances or even command type |
| codec/context_map     | Encode/decode the brotli context_map which remaps the previous 6 bits and literal_block_type to a prior between 0 and 255 |
| codec/literal         | Encode/decode new raw data that appears in the file. This can use a number of strategies or combinations of strategies to encode each nibble |
| codec/priors          | Structs defining the size of the tables that contain dynamically-trained CDF holding statistics about past-data. |
| codec/weights         | struct that blend between multiple CDFs based on prior efficacy |
| codec/specializations | Optimization system to generate separate codepaths for currently-running nibble-decode or encode path, based on which priors were selected |
| codec                 | Encode/decode the overall commands themselves and track the state of the compression of the overall file and if it is complete |
| divans_decompressor   | Implementation of Decompressor trait that parses divans headers and translates the ANS stream into commands and into raw data |
| brotli_ir_gen         | Implementation of Compressor trait that calls into the brotli codec and extracts the command array per metablock to be encoded |
| divans_compressor     | Alternate implementation of Compressor trait that calls into raw_to_cmd instead of brotli to get the command array per metablock |
| divans_to_raw         | DecoderSpecialization for the codec to assume default input commands and incrementally populate them |
| cmd_to_divans         | EncoderSpecialization for the codec to take input commands and produce divans |
| raw_to_cmd            | Future: a substitute for the Brotli compressor to generate commands |
| cmd_to_raw            | Interpret a list of Brotli commands and produce the uncompressed file |
| arithmetic_coder      | Define EntropyEncoder and EntropyDecoder arithmetic coder traits |
| ans                   | Fast implementation of EntropyEncoder and EntropyDecoder interfaces |
| billing               | Plugin to add attribution to an ArithmeticEncoderOrDecoder by providing the same interface and wrapping the en/decoder |
| alloc_util            | Allocator that reuses a single slice of memory over many allocations |
| slice_util            | A mechanism to borrow and reference an existing slice that can be frozen, unborrowing the slice, when divans returns to the caller to request more input or output space |
| resizable_buffer      | Simple resizing byte buffer that can hold the raw input and output streams being processed |
| reader                | Read implementation for both encoding and decoding of divans |
| writer                | Write implementation for both encoding and decoding of divans |

## Overall flow

### To Encode a file,

* a `writer::DivansBrotliHybridCompressorWriter` instantiates a `brotli_ir_gen::BrotliDivansHybridCompressor`
* The compressor has both a `brotli::BrotliEncoderStateStruct` from the brotli crate as well as a `codec::DivansCodec<ANSEncoder, EncodeSpecialization>`.
* Using `brotli_ir_gen::BrotliDivansHybridCompressor::encode`, the compressor feeds input data into the `brotli::BrotliEncoderStateStruct`
  * by calling `brotli::BrotliEncoderCompressStream`
* `brotli::BrotliEncoderCompressStream` can trigger a callback into `brotli_ir_gen::BrotliDivansHybridCompressor::divans_encode_commands`
  * The callback will consist of a slice of `brotli::interface::Command` items
  * These items are fed into the `codec::DivansCodec<ANSEncoder, EncodeSpecialization>::encode_or_decode`, which encodes them into divans format.
     * `codec::DivansCodec<ANSEncoder, cmd_to_divans::EncoderSpecialization>::encode_or_decode` accomplishes this by using the `EncoderSpecialization` to pull input commands as the source of truth
     * unfortunately brotli can pass as much data as it wishes to the caller, up to the maximum metablock size of 16 megs.
       * this means the caller has to buffer this data in a `resizable_buffer::ResizableBuffer`
* When all the callbacks have completed, `brotli_ir_gen::BrotliDivansHybridCompressor::encode_stream` does its best to flush the raw buffer
* Eventually when the user calls `brotli_ir_gen::BrotliDivansHybridCompressor::flush` a similar procedure is followed but with finish flags set

### To Decode a file,

* a `reader::DivansDecompressorReader` instantiates a `divans_decompressor::DivansDecompressor`
* The decompressor is an enum that switches from `HeaderParser` mode into `Decode` mode after the 16 byte raw header has been parsed
* `divans_decompressor::DivansDecompressor::Decode` has a `codec::DivansCodec<ANSDecoder, DecoderSpecialization>` within.
* Using `brotli_ir_gen::DivansDeompressor::decode`, the decompressor feeds input data directly into `codec::DivansCodec<ANSDecoder, DecoderSpecialization>`
  * The codec.encode_or_decode is designed to receive commands as input when encoding, so the `divans_to_raw::DecoderSpecialization` simply makes placeholder commands for each type of command so that the same codepath can encode and decode commands
* when a final state is reached, a checksum is written and success is returned

## The codec state machine

`codec::DivansCodec` has three members, `cross_command_state`, which tracks the probability models, the `state`, to track which kind of command is being decoded, and `codec_traits`, used as a repository of compiler constant values that happen to be set that way during this decode or encode phase based on the header and command data.

The `state` value is an enumerant that can either carry command-specific information or can mark that the ring buffer must be populated, etc.

### Overview of the available codec states
* `Begin`: This state means that the decoder is not in the middle of coding a particular command, so the next step will be to decode what the next command is
* `Literal(literal::LiteralState`: the coder is in the process of coding raw literals to be injected into the file
* `Dict(dict::DictState)`: the coder is in the process of coding a word that appears in the brotli dictionary
* `Copy(copy::CopyState)`: the coder is in the process of coding a reference to pull data from the ring buffer
* `BlockSwitchLiteral(block_type::LiteralBlockTypeState)`: The coder was instructed to serialize an arbitrary value that will affect how the predictor models future literals
* `BlockSwitchCommand(block_type::BlockTypeState)`: The coder was instructed to serialize an arbitrary value that will affect how the predictor models nothing (TODO)
* `BlockSwitchDistance(block_type::BlockTypeState)`: The coder was instructed to serialize an arbitrary value that will affect how the predictor models distances to copy from and dictionary values.
* `PredictionMode(context_map::PredictionModeState)`:  The coder was instructed to serialize out a context map that remaps the BlockSwitchLiteral value plus the last 6 bits into a value in [0, 255] that is used as an index into the array of CDFs to be trained
* `PopulateRingBuffer(Command<AllocatedMemoryPrefix<u8, AllocU8>>)` When Literal, Dict, or Copy states reach their termination state, those states are moved into the PopulateRingBuffer state.
  * `PopulateRingBuffer` uses the `cmd_to_raw::DivansRecoderState` stored in `DivanCodec::CrossCommandState` to populate the ring buffer
    * If DecoderSpecialization is selected, `cmd_to_raw::DivansRecoderState` copies the data to the output bytes, returning and requesting NeedBytes until all bytes have been serialized
    * Otherwise the EncoderSpecialization avoids serializing those bytes.
  * After all necessary bytes were serialized and the ring buffer populated, then the last_8_literals are saved to be used as future priors
* `WriteChecksum(usize)` This state happens if an end command (0xf) is encountered during a decode or a `code::DivansCodec::flush` happens on encode
  * Currently checksum support is not active, but 8 bytes are simply serialized
* `DivansSuccess` This state is reached when WriteChecksum is complete on the decoder or when the final command is reached on the encoder
* `EncodedShutdownNode` | `ShutdownCoder` | `CoderBufferDrain` appear only in teh encoder during flush/close after the EOF node type as flushed



# Acknowledgements

Special thanks to Jaroslaw (Jarek) Duda and Fabian Giesen for genius work and their detailed and thoughtful presentation of the ANS algorithm.

