char * find_first_arg(int argc, char**argv) {
    int i;
    for (i = 1; i < argc; ++i) {
        if (argv[i][0] != '-') {
            return argv[i];
        }
    }
    return NULL;
}
void set_options(struct DivansCompressorState *state, int argc, char **argv) {
    int i;
    unsigned int ret =0 ;
    int used_cm = 0;
    for (i = 1; i < argc; ++i) {
        if (strstr(argv[i], "-q") == argv[i]) {
            ret = divans_set_option(state, DIVANS_OPTION_QUALITY, atoi(argv[i] + 2));
            assert(ret == DIVANS_SUCCESS);
        }
        if (strstr(argv[i], "-p") == argv[i]) {
            ret = divans_set_option(state, DIVANS_OPTION_PRIOR_BITMASK_DETECTION, atoi(argv[i] + 2));
            assert(ret == DIVANS_SUCCESS);
        }
        if (strstr(argv[i], "-l") == argv[i]) {
            ret = divans_set_option(state, DIVANS_OPTION_USE_BROTLI_COMMAND_SELECTION, 0);
            assert(ret == DIVANS_SUCCESS);
        }
        if (strstr(argv[i], "-w") == argv[i]) {
            ret = divans_set_option(state, DIVANS_OPTION_WINDOW_SIZE, atoi(argv[i] + 2));
            assert(ret == DIVANS_SUCCESS);
        }
        if (strstr(argv[i], "-a") == argv[i]) {
            ret = divans_set_option(state, DIVANS_OPTION_LITERAL_ADAPTATION_CM_HIGH, atoi(argv[i] + 2));
            assert(ret == DIVANS_SUCCESS);
        }
        if (strstr(argv[i], "-cm") == argv[i]) {
            used_cm = 1;
            ret = divans_set_option(state, DIVANS_OPTION_USE_CONTEXT_MAP, 1);
            assert(ret == DIVANS_SUCCESS);
            if (argv[i] + 3 !='\0') {
                ret = divans_set_option(state, DIVANS_OPTION_FORCE_LITERAL_CONTEXT_MODE, atoi(argv[i] + 3));
                assert(ret == DIVANS_SUCCESS);
            }
        }
        if (strstr(argv[i], "-bs") == argv[i]) {
            ret = divans_set_option(state, DIVANS_OPTION_STRIDE_DETECTION_QUALITY, 1);
            assert(ret == DIVANS_SUCCESS);
        }
        if (strstr(argv[i], "-as") == argv[i]) {
            ret = divans_set_option(state, DIVANS_OPTION_STRIDE_DETECTION_QUALITY, 2);
            assert(ret == DIVANS_SUCCESS);
        }
    }
    for (i = 1; i < argc; ++i) {
        if (strstr(argv[i], "-s") == argv[i]) {
            if (used_cm) {
                ret = divans_set_option(state, DIVANS_OPTION_DYNAMIC_CONTEXT_MIXING, 1);
                assert(ret == DIVANS_SUCCESS);
            }
            if (strcmp(argv[i], "-s") != 0) { // diff
                ret = divans_set_option(state, DIVANS_OPTION_FORCE_STRIDE_VALUE, atoi(argv[i]+2));
                assert(ret == DIVANS_SUCCESS);                
            }
        }
    }
    for (i = 1; i < argc; ++i) {
        if (strstr(argv[i], "-m") == argv[i]) {
            if (strcmp(argv[i], "-m") != 0) { // diff
                ret = divans_set_option(state, DIVANS_OPTION_DYNAMIC_CONTEXT_MIXING, atoi(argv[i]+2));
            } else {
                ret = divans_set_option(state, DIVANS_OPTION_DYNAMIC_CONTEXT_MIXING, 2);
            }
            assert(ret == DIVANS_SUCCESS);
        }
    }
}
