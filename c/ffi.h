#ifndef _DIVANS_H_
#define _DIVANS_H_
#include <stdint.h>
#include <string.h>
const unsigned char * hello_rust();
struct CAllocator {
    void* (*alloc_func)(void * opaque, size_t data);
    void (*free_func)(void * opaque, void * mfd);
    void * opaque;
};
struct DivansDecompressorState;
struct DivansCompressorState;
struct DivansCompressorState* new_compressor_with_custom_alloc(struct CAllocator alloc);
void free_compressor(struct DivansCompressorState* mfd);
#endif
