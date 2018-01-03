int use_real_malloc = 1;
int use_fake_malloc = 0;
void* custom_alloc_opaque = &use_real_malloc;
unsigned char huge_buffer[1024*1024 * 255];
size_t huge_buffer_offset = 0;
const uint32_t science = 0x5C1E11CE;
void * custom_malloc_f(void* opaque, size_t data) {
    void * retval;
    size_t amt = data + 2*sizeof(opaque) + 4;
    if (opaque == &use_fake_malloc) {
        retval = &huge_buffer[huge_buffer_offset];
        huge_buffer_offset += amt;
    } else {
        retval = malloc(amt);
    }
    memcpy(retval, &science, 4);
    memcpy((char*)retval + 4, &opaque, sizeof(opaque));
    memcpy((char*)retval + 4 + sizeof(opaque), &data, sizeof(size_t));
    return retval + sizeof(opaque) + sizeof(size_t) + 4;
}
void * (*custom_malloc)(void* opaque, size_t data) = &custom_malloc_f;
void custom_free_f(void* opaque, void *mfd) {
    void * local_opaque;
    uint32_t local_science;
    size_t local_size = 0;
    char * local_mfd = (char *)mfd;
    if (mfd == NULL) {
        return;
    }
    local_mfd -= 4;
    local_mfd -= sizeof(opaque);
    local_mfd -= sizeof(size_t);
    memcpy(&local_science, local_mfd, 4);
    assert(local_science == science);
    memcpy(&local_opaque, local_mfd + 4, sizeof(opaque));
    memcpy(&local_size, local_mfd + 4 + sizeof(opaque), sizeof(size_t));
    assert(opaque == local_opaque);
    if (opaque == &use_fake_malloc) {
        void *retval = &huge_buffer[huge_buffer_offset];
        if ((void*)(retval - local_size) == mfd) {
            huge_buffer_offset -= 4 + sizeof(opaque) + sizeof(size_t) + local_size;
        }
    } else {
        free(local_mfd);
    }
}

void (*custom_free)(void* opaque, void *mfd) = &custom_free_f;
void custom_atoi(char * dst, size_t data) {
    if (!data) {
        memcpy(dst, "0\0", 2);
        return;
    }
    char *ptr = dst;
    while(data) {
        *ptr = '0' + (data % 10);
        ++ptr;
        data /= 10;
    }
    *ptr = '\0';
    int del = (int)(ptr - dst);
    int i;
    for (i = 0;i < del/2;i+= 1) {
        char tmp = dst[i];
        dst[i] = *(ptr - i - 1);
        *(ptr - i - 1) = tmp;
    }
}
