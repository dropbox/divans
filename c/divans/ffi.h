#ifndef _DIVANS_H_
#define _DIVANS_H_
#include <stdint.h>
#include <string.h>

typedef uint8_t DivansResult;

#define DIVANS_SUCCESS ((uint8_t)0)
#define DIVANS_NEEDS_MORE_INPUT ((uint8_t)1)
#define DIVANS_NEEDS_MORE_OUTPUT ((uint8_t)2)
#define DIVANS_FAILURE ((uint8_t)3)

typedef uint8_t DivansOptionSelect;

#define DIVANS_OPTION_QUALITY 1
#define DIVANS_OPTION_WINDOW_SIZE 2
#define DIVANS_OPTION_LGBLOCK 3
#define DIVANS_OPTION_DYNAMIC_CONTEXT_MIXING 4
#define DIVANS_OPTION_USE_BROTLI_COMMAND_SELECTION 5
#define DIVANS_OPTION_USE_BROTLI_BITSTREAM 6
#define DIVANS_OPTION_USE_CONTEXT_MAP 7
#define DIVANS_OPTION_LITERAL_ADAPTATION 8
#define DIVANS_OPTION_FORCE_STRIDE_VALUE 9
#define DIVANS_OPTION_STRIDE_DETECTION_QUALITY 10

/// a struct specifying custom allocators for divans to use instead of the builtin rust allocators.
/// if all 3 values are set to NULL, the Rust allocators are used instead.
struct CAllocator {
    /// Allocate length bytes. The returned pointer must be 32-byte aligned unless divans was built without features=simd
    void* (*alloc_func)(void * opaque, size_t length);
    void (*free_func)(void * opaque, void * mfd);
    void * opaque;
};
struct DivansDecompressorState;
struct DivansCompressorState;

struct DivansCompressorState* divans_new_compressor();
struct DivansCompressorState* divans_new_compressor_with_custom_alloc(struct CAllocator alloc);
DivansResult divans_set_option(struct DivansCompressorState* state, DivansOptionSelect selector, uint32_t value);
DivansResult divans_encode(struct DivansCompressorState* state,
                           const uint8_t *input_buf_ptr, size_t input_size, size_t*input_offset,
                           uint8_t *output_buf_ptr, size_t output_size, size_t *output_offset);

DivansResult divans_encode_flush(struct DivansCompressorState* state,
                                 uint8_t *output_buf_ptr, size_t output_size, size_t *output_offset);

void divans_free_compressor(struct DivansCompressorState* mfd);


struct DivansDecompressorState* divans_new_decompressor();
struct DivansDecompressorState* divans_new_decompressor_with_custom_alloc(struct CAllocator alloc);
DivansResult divans_decode(struct DivansDecompressorState* state,
                           const uint8_t *input_buf_ptr, size_t input_size, size_t*input_offset,
                           uint8_t *output_buf_ptr, size_t output_size, size_t *output_offset);

void divans_free_decompressor(struct DivansDecompressorState* mfd);



#endif
