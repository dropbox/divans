#ifndef _DIVANS_H_
#define _DIVANS_H_
#include <stdint.h>
#include <string.h>

typedef unsigned char DivansResult;

#define DIVANS_SUCCESS ((unsigned char)0)
#define DIVANS_NEEDS_MORE_INPUT ((unsigned char)1)
#define DIVANS_NEEDS_MORE_OUTPUT ((unsigned char)2)
#define DIVANS_FAILURE ((unsigned char)3)

struct CAllocator {
    void* (*alloc_func)(void * opaque, size_t data);
    void (*free_func)(void * opaque, void * mfd);
    void * opaque;
};
struct DivansDecompressorState;
struct DivansCompressorState;

struct DivansCompressorState* divans_new_compressor();
struct DivansCompressorState* divans_new_compressor_with_custom_alloc(struct CAllocator alloc);
DivansResult divans_encode(struct DivansCompressorState* state,
                           const unsigned char *input_buf_ptr, size_t input_size, size_t*input_offset,
                           unsigned char *output_buf_ptr, size_t output_size, size_t *output_offset);

DivansResult divans_encode_flush(struct DivansCompressorState* state,
                                 unsigned char *output_buf_ptr, size_t output_size, size_t *output_offset);

void divans_free_compressor(struct DivansCompressorState* mfd);


struct DivansDecompressorState* divans_new_decompressor();
struct DivansDecompressorState* divans_new_decompressor_with_custom_alloc(struct CAllocator alloc);
DivansResult divans_decode(struct DivansDecompressorState* state,
                           const unsigned char *input_buf_ptr, size_t input_size, size_t*input_offset,
                           unsigned char *output_buf_ptr, size_t output_size, size_t *output_offset);

void divans_free_decompressor(struct DivansDecompressorState* mfd);



#endif
