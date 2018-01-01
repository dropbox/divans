#include "ffi.h"
int main() {
    struct CAllocator alloc = {NULL, NULL, NULL};
    struct DivansCompressorState *alloced = new_compressor_with_custom_alloc(alloc);
    free_compressor(alloced);
    return 0;
}
