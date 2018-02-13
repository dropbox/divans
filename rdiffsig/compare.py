import Queue
import subprocess
import sys
import tempfile
import threading
import zlib

output_queue = Queue.Queue()
def print_results():
    printed_header = False
    while True:
        val = output_queue.get()
        if not val:
            return
        if not printed_header:
            print ','.join([key for key in sorted(val.keys())])
            printed_header = True
        print ','.join([str(val[key]) for key in sorted(val.keys())])
        sys.stdout.flush()

def find_binary(fallback, loc):
    try:
        with open(loc):
            return loc
    except Exception:
        return fallback

BROTLI_BIN=find_binary('/srv/compression-benchmark/bin/dict-brotli', './brotli')
BROTLI_DEC=find_binary(BROTLI_BIN, './brotli-decompressor')
DIVANS_BIN=find_binary('/srv/compression-benchmark/bin/dict-divans', './dict-divans')
RDIFF_BIN=find_binary('/srv/compression-benchmark/bin/rdiff', './rdiff')
RDIFFSIG_BIN=find_binary('/srv/compression-benchmark/bin/rdiffsig', './rdiffsig')
DIVANS_ARGS=['-c','-cm', '-s', '-mixing=2', '-lgwin=24', '-window=24']
BROTLI_ARGS=['-c','-l24', '-w24']
ONE_BLOCK='one'
SINGLE_BLOCK='sig'
MULTI_BLOCK='mul'
PERMISSIVE = True
num_bad = 0
def validate_permissive(condition, good, fallback, kind, name, datafiles):
    global num_bad
    if not PERMISSIVE:
        assert condition
    if condition:
        return good
    sys.stderr.write(str(name) + " failed to validate..."+ str(kind)+ " falling back ")
    with open('bad_'+str(num_bad), 'w') as first:
        with open('bad_dict_'+str(num_bad), 'w') as second:
            first.write(datafiles[0])
            second.write(datafiles[1])
    num_bad += 1
    return fallback
def compare_algo(old_file, new_file, block_size, crypto_bytes,
                 add_blocks=ONE_BLOCK, ext='.unk', name=None):

    PIPE = subprocess.PIPE
    rdiff_sig = subprocess.Popen([RDIFFSIG_BIN,
                                  '-sig' + str(crypto_bytes),
                                  '-blocksize='+str(block_size)],
                                 stdout=PIPE, stdin=PIPE)
    sig_file, _stderr = rdiff_sig.communicate(old_file)
    ret = rdiff_sig.wait()
    assert not ret
    with tempfile.NamedTemporaryFile(delete=True) as sig_fd:
        sig_fd.write(sig_file)
        sig_fd.flush()
        with tempfile.NamedTemporaryFile(delete=True) as dict_file:
            with tempfile.NamedTemporaryFile(delete=True) as mask_file:
                rdiff_sig = subprocess.Popen([RDIFFSIG_BIN,
                      '-sig='+sig_fd.name,
                      '-dict='+ dict_file.name,
                      '-dictmask=' + mask_file.name,
                ], stdin=PIPE)
                _stdout, _stderr = rdiff_sig.communicate(new_file)
                ret = rdiff_sig.wait()
                assert not ret
                rdiff_proc = subprocess.Popen([RDIFF_BIN, 'delta', sig_fd.name],
                                              stdout=PIPE, stdin=PIPE)
                rdiff_delta, _stderr = rdiff_proc.communicate(new_file)
                ret = rdiff_proc.wait()
                assert not ret
                with tempfile.NamedTemporaryFile(delete=True) as old_file_fd:
                    old_file_fd.write(old_file)
                    old_file_fd.flush()
                    rdiff_proc = subprocess.Popen([RDIFF_BIN, 'patch', old_file_fd.name],
                                                  stdout=PIPE, stdin=PIPE)
                    candidate_new_file, _stderr = rdiff_proc.communicate(rdiff_delta)
                    if candidate_new_file != new_file:
                        print 'fallback for block_size=',block_size,'bytes=',crypto_bytes
                        if crypto_bytes == 8:
                            assert candidate_new_file == new_file
                        elif crypto_bytes == 4:
                            # recursively try more crypto bytes
                            return compare_algo(old_file,
                                                new_file,
                                                block_size,
                                                8,
                                                add_blocks,
                                                ext,
                                                name)
                        else:
                            return compare_algo(old_file,
                                                new_file,
                                                block_size,
                                                crypto_bytes + 1,
                                                add_blocks,
                                                ext,
                                                name)
                    # just zlib the rsync-created delta
                    zlib_delta = zlib.compress(rdiff_delta)
                    zlib9_delta = zlib.compress(rdiff_delta, 9)
                    # try brotli using the same delta
                    brotli_proc = subprocess.Popen([BROTLI_BIN] + BROTLI_ARGS,
                                                   stdout=PIPE,
                                                   stdin=PIPE)
                    brotli_delta_out, _stderr = brotli_proc.communicate(rdiff_delta)
                    brotli_delta_out = validate_permissive(not brotli_proc.wait(),
                                                           brotli_delta_out,
                                                           zlib_delta,
                                                           'brotli_delta',
                                                           name,
                                                           (rdiff_delta, old_file))
                    brotli_proc = subprocess.Popen([BROTLI_DEC],
                                                   stdout=PIPE,
                                                   stdin=PIPE)
                    # and lastly, divans on the same delta
                    brotli_delta_rt, _stderr = brotli_proc.communicate(brotli_delta_out)
                    brotli_delta_out = validate_permissive(brotli_delta_rt == rdiff_delta,
                                                           brotli_delta_out,
                                                           zlib_delta,
                                                           'brotli_delta',
                                                           name,
                                                           (rdiff_delta, old_file))

                    divans_proc = subprocess.Popen([DIVANS_BIN] + DIVANS_ARGS,
                                                   stdout=PIPE, stdin=PIPE)
                    divans_delta_out, _stderr = divans_proc.communicate(rdiff_delta)
                    divans_delta_out = validate_permissive(not divans_proc.wait(),
                                                           divans_delta_out,
                                                           zlib_delta,
                                                           'divans_delta',
                                                           name,
                                                           (rdiff_delta, old_file))
                    divans_proc = subprocess.Popen([DIVANS_BIN],
                                                   stdout=PIPE, stdin=PIPE)
                    divans_delta_rt, _stderr = divans_proc.communicate(divans_delta_out)
                    divans_delta_out = validate_permissive(divans_delta_rt == rdiff_delta,
                                                           divans_delta_out,
                                                           zlib_delta,
                                                           'divans_delta',
                                                           name,
                                                           (rdiff_delta, old_file))

                    # try brotli as if it were a download and we both had full old block
                    brotli_proc = subprocess.Popen([BROTLI_BIN,
                                                    '-dict=' + old_file_fd.name] + BROTLI_ARGS,
                                                   stdout=PIPE,
                                                   stdin=PIPE)
                    brotli_dl_out, _stderr = brotli_proc.communicate(new_file)
                    brotli_dl_out = validate_permissive(not brotli_proc.wait(),
                                                        brotli_dl_out,
                                                        brotli_delta_out,
                                                        'brotli_dl',
                                                        name,
                                                        (new_file, old_file))
                    brotli_proc = subprocess.Popen([BROTLI_DEC, '-dict='+old_file_fd.name],
                                                   stdout=PIPE, stdin=PIPE)
                    brotli_rt, _stderr = brotli_proc.communicate(brotli_dl_out)
                    brotli_dl_out = validate_permissive(brotli_rt == new_file,
                                                        brotli_dl_out,
                                                        brotli_delta_out,
                                                        'brotli_dl',
                                                        name,
                                                        (new_file, old_file))



                    divans_proc = subprocess.Popen([DIVANS_BIN,
                                                    '-dict=' + old_file_fd.name] + DIVANS_ARGS,
                                                   stdout=PIPE,
                                                   stdin=PIPE)
                    divans_dl_out, _stderr = divans_proc.communicate(new_file)
                    divans_dl_out = validate_permissive(not divans_proc.wait(),
                                                        divans_dl_out,
                                                        divans_delta_out,
                                                        'divans_dl',
                                                        name,
                                                        (new_file, old_file))
                    divans_proc = subprocess.Popen([DIVANS_BIN, '-dict='+old_file_fd.name],
                                                   stdout=PIPE, stdin=PIPE)
                    divans_rt, _stderr = divans_proc.communicate(divans_dl_out)
                    divans_dl_out = validate_permissive(divans_rt == new_file,
                                                        divans_dl_out,
                                                        divans_delta_out,
                                                        'divans_dl',
                                                        name,
                                                        (new_file, old_file))



                    addendum = 8
                    if len(old_file) % block_size != 0:
                        addendum += block_size - (len(old_file) % block_size)
                    # pad the old file to be dictionary-like and have multiple
                    old_file_fd.write(chr(0) * addendum)
                    old_file_fd.flush()

                    divans_proc = subprocess.Popen([DIVANS_BIN] + DIVANS_ARGS + [
                        '-dict=' + dict_file.name,
                        '-dictmask=' + mask_file.name], stdout=PIPE, stdin=PIPE)
                    divans_dict_out, _stderr = divans_proc.communicate(new_file)
                    divans_dict_out = validate_permissive(not divans_proc.wait(),
                                                          divans_dict_out,
                                                          divans_delta_out,
                                                          'divans_dict',
                                                          name,
                                                          (new_file, old_file))
                    divans_proc = subprocess.Popen([DIVANS_BIN, '-dict='+old_file_fd.name],
                                                   stdout=PIPE, stdin=PIPE)
                    divans_rt, _stderr = divans_proc.communicate(divans_dict_out)
                    divans_dict_out = validate_permissive(divans_rt == new_file,
                                                          divans_dict_out,
                                                          divans_delta_out,
                                                          'divans_dict',
                                                          name,
                                                          (new_file, old_file))
                    brotli_proc = subprocess.Popen([BROTLI_BIN,
                                                    '-dict=' + dict_file.name,
                                                    '-dictmask=' + mask_file.name] + BROTLI_ARGS,
                                                   stdout=PIPE,
                                                   stdin=PIPE)
                    brotli_dict_out, _stderr = brotli_proc.communicate(new_file)
                    brotli_dict_out = validate_permissive(not brotli_proc.wait(),
                                                          brotli_dict_out,
                                                          brotli_delta_out,
                                                          'brotli_dict',
                                                          name,
                                                          (new_file, old_file))
                    brotli_proc = subprocess.Popen([BROTLI_DEC, '-dict='+old_file_fd.name],
                                                   stdout=PIPE, stdin=PIPE)
                    brotli_rt, _stderr = brotli_proc.communicate(brotli_dict_out)
                    brotli_dict_out = validate_permissive(brotli_rt == new_file,
                                                          brotli_dict_out,
                                                          brotli_delta_out,
                                                          'brotli_dict',
                                                          name,
                                                          (new_file, old_file))


                    zlib9_raw_out = zlib.compress(new_file, 9)
                    zlib_raw_out = zlib.compress(new_file)
                    brotli_proc = subprocess.Popen([BROTLI_BIN] + BROTLI_ARGS,
                                                   stdout=PIPE,
                                                   stdin=PIPE)
                    brotli_raw_out, _stderr = brotli_proc.communicate(new_file)
                    brotli_proc = subprocess.Popen([BROTLI_DEC],
                                                   stdout=PIPE,
                                                   stdin=PIPE)
                    brotli_raw_rt, _stderr = brotli_proc.communicate(brotli_raw_out)
                    brotli_raw_out = validate_permissive(brotli_raw_rt == new_file,
                                                         brotli_raw_out,
                                                         zlib_raw_out,
                                                         'brotli_raw',
                                                         name,
                                                         (new_file, old_file))
                    results = {
                        'addblocks': add_blocks,
                        'bits_sig': crypto_bytes * 8,
                        'block_size':  block_size,
                        'dict_divans': len(divans_dict_out),
                        'dict_brotli': len(brotli_dict_out),
                        'delta_divans': len(divans_delta_out),
                        'delta_brotli': len(brotli_delta_out),
                        'delta_zlib': len(zlib_delta),
                        'delta_zlib9': len(zlib9_delta),
                        'dl_brotli': len(brotli_dl_out),
                        'dl_divans': len(divans_dl_out),
                        'raw_brotli': len(brotli_raw_out),
                        'raw_zlib': len(zlib_raw_out),
                        'raw_zlib9': len(zlib9_raw_out),
                        'xid': name,
                        'xt': ext,
                        }
                    global output_queue
                    output_queue.put(results)

def compare_algorithms(old_file, new_file, add_blocks=ONE_BLOCK, ext='.unk', name=None):
    for block_size in (128, 256, 512, 1024, 2048):
        compare_algo(old_file, new_file, block_size, 4, add_blocks, ext, name)

def main(old_file_name, new_file_name):
    t = threading.Thread(target=print_results)
    t.start()
    with open(old_file_name) as of:
        with open(new_file_name) as nf:
            compare_algorithms(of.read(), nf.read())
            #compare_algo(of.read(), nf.read(), 256, 4)
    output_queue.put(None)
    t.join()
if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
