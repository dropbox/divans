#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <assert.h>
#ifndef _WIN32
#include <unistd.h>
#endif
#include "ffi.h"
#include "arg.h"
#include "custom_alloc.h"
#include "vec_u8.h"
const unsigned char example[]=
    "Mary had a little lamb. Its fleece was white as snow.\n"
    "And every where that Mary went, the lamb was sure to go.\n"
    "It followed her to school one day which was against the rule.\n"
    "It made the children laugh and play to see a lamb at sch00l!\n\n\n\n"
    "0 1 1 2 3 5 8 13 21 34 55 89 144 233 377 610 987 1597 2584 4181 6765\n"
    "\x11\x99\x2f\xfc\xfe\xef\xff\xd8\xfd\x9c\x43"
    "Additional testing characters here";



#define BUF_SIZE 65536
DivansResult compress(const unsigned char *data, size_t len, struct VecU8 *ret_buffer,
                      int argc, char** argv) {
    unsigned char buf[BUF_SIZE];
    struct CAllocator alloc = {custom_malloc, custom_free, custom_alloc_opaque};
    struct DivansCompressorState *state = divans_new_compressor_with_custom_alloc(alloc);
    set_options(state, argc, argv);
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
        push_vec_u8(ret_buffer, buf, buf_offset);
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
        push_vec_u8(ret_buffer, buf, buf_offset);
    } while(res != DIVANS_SUCCESS);
    divans_free_compressor(state);
    return DIVANS_SUCCESS;
}

DivansResult decompress(const unsigned char *data, size_t len, struct VecU8 *ret_buffer) {
    unsigned char buf[BUF_SIZE];
    struct CAllocator alloc = {custom_malloc, custom_free, custom_alloc_opaque};
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
        push_vec_u8(ret_buffer, buf, buf_offset);
    } while (res != DIVANS_SUCCESS);
    divans_free_decompressor(state);
    return DIVANS_SUCCESS;
}

int main(int argc, char**argv) {
    custom_free_f(&use_fake_malloc, memset(custom_malloc_f(&use_fake_malloc, 127), 0x7e, 127));
    if (getenv("NO_MALLOC")) {
        custom_alloc_opaque = &use_fake_malloc;
    }
    if (getenv("RUST_MALLOC")) {
        custom_alloc_opaque = NULL;
        custom_malloc = NULL;
        custom_free = NULL;
    }
    const unsigned char* data = example;
    size_t len = sizeof(example);
    unsigned char* to_free = NULL;
    if (find_first_arg(argc, argv)) {
        FILE * fp = fopen(find_first_arg(argc, argv), "rb");
        if (fp != NULL) {
            (void)fseek(fp, 0, SEEK_END);
            len = ftell(fp);
            (void)fseek(fp, 0, SEEK_SET);
            to_free = malloc(len);
            (void)fread(to_free, 1, len, fp);
            data = to_free;
            (void)fclose(fp);
        }
    }
    {
        struct VecU8 divans_file = new_vec_u8();
        struct VecU8 rt_file = new_vec_u8();
        DivansResult res = compress(data, len, &divans_file, argc, argv);
        if (res != DIVANS_SUCCESS) {
            fprintf(stderr, "Failed to compress code:%d\n", (int) res);
            abort();
        }
        res = decompress(divans_file.data, divans_file.size, &rt_file);
        if (res != DIVANS_SUCCESS) {
            fprintf(stderr, "Failed to compress code:%d\n", (int)res);
            abort();
        }
        if (rt_file.size != len) {
            FILE * fp = fopen("/tmp/fail.rt", "wb");
            fwrite(rt_file.data, 1, rt_file.size, fp);
            fclose(fp);
            fp = fopen("/tmp/fail.dv", "wb");
            fwrite(divans_file.data, 1, divans_file.size, fp);
            fclose(fp);
            fp = fopen("/tmp/fail.or", "wb");
            fwrite(data, 1, len, fp);
            fclose(fp);
            fprintf(stderr, "Decompressed file size %ld != %ld\n", (long) rt_file.size, (long)len);
            abort();
        }
        if (memcmp(rt_file.data, data, len) != 0) {
            fprintf(stderr, "Roundtrip Contents mismatch\n");
            abort();
        }
#ifdef _WIN32
        printf("File length %ld reduced to %ld, %0.2f%%\n",
               (long)len, (long)divans_file.size,(double)divans_file.size * 100.0 / (double)len);
#else
        char buf[512];
        (void)write(1, "File length ", strlen("File Length "));
        custom_atoi(buf, len);
        (void)write(1, buf, strlen(buf));
        (void)write(1, " reduced to ", strlen(" reduced to "));
        custom_atoi(buf, divans_file.size);
        (void)write(1, buf, strlen(buf));
        (void)write(1, ", ", strlen(", "));
        custom_atoi(buf, divans_file.size * 100 / len);
        (void)write(1, buf, strlen(buf));
        (void)write(1, ".", strlen("."));
        custom_atoi(buf, ((divans_file.size * 1000000 + len/2)/ len) % 10000 + 10000);
        (void)write(1, buf + 1, strlen(buf) - 1);
        (void)write(1, "%\n", strlen("%\n"));
#endif
        release_vec_u8(&divans_file);
        release_vec_u8(&rt_file);
    }
    if (to_free != NULL) {
        free(to_free);
    }
    return 0;
}
