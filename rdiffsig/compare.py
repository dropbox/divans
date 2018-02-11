import Queue
import sys
import zlib
import subprocess
import tempfile
import threading

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
BROTLI_BIN='./brotli'
DIVANS_BIN='./divans'
RDIFF_BIN='./rdiff'
RDIFFSIG_BIN='./rdiffsig'
DIVANS_ARGS=['-c','-cm', '-s', '-mixing=2', '-speed=1,1024']
def compare_algo(old_file, new_file, block_size, crypto_bytes):
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
                rdiff_proc = subprocess.Popen([RDIFF_BIN, 'delta', sig_fd.name], stdout=PIPE, stdin=PIPE)
                rdiff_delta, _stderr = rdiff_proc.communicate(new_file)
                ret = rdiff_proc.wait()
                assert not ret
                with tempfile.NamedTemporaryFile(delete=True) as old_file_fd:
                    old_file_fd.write(old_file)
                    old_file_fd.flush()
                    rdiff_proc = subprocess.Popen([RDIFF_BIN, 'patch', old_file_fd.name], stdout=PIPE, stdin=PIPE)
                    candidate_new_file, _stderr = rdiff_proc.communicate(rdiff_delta)
                    if candidate_new_file != new_file:
                        print 'fallback for block_size=',block_size,'bytes=',crypto_bytes
                        if crypto_bytes == 8:
                            assert candidate_new_file == new_file
                        elif crypto_bytes == 4:
                            # recursively try more crypto bytes
                            return compare_algo(old_file, new_file, block_size, 8)
                        else:
                            return compare_algo(old_file, new_file, block_size, crypto_bytes + 1)
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
                    ret = divans_proc.wait()
                    assert not ret
                    #print old_file_fd.tell()
                    #print len(dict_file.read())
                    #with open('../doctoreddict256') as atest:
                    #    old_file_fd.seek(0)
                    #    assert atest.read() == old_file_fd.read()
                    #with open('/tmp/256.mask') as atest:
                    #    xx=atest.read()
                    #    yy=mask_file.read()
                    #    sys.stdout.write(yy)
                    #    assert xx == yy
                    #with open('/tmp/256.dict') as atest:
                    #    assert atest.read() == dict_file.read()
                    divans_proc = subprocess.Popen([DIVANS_BIN, '-dict='+old_file_fd.name],
                                                   stdout=PIPE, stdin=PIPE)
                    divans_rt, _stderr = divans_proc.communicate(divans_dict_out)
                    assert divans_rt == new_file
                    brotli_proc = subprocess.Popen([BROTLI_BIN, '-c',
                                                    '-dict=' + dict_file.name,
                                                    '-dictmask=' + mask_file.name],
                                                   stdout=PIPE,
                                                   stdin=PIPE)
                    brotli_dict_out, _stderr = brotli_proc.communicate(new_file)
                    brotli_proc = subprocess.Popen([BROTLI_BIN, '-dict='+old_file_fd.name],
                                                   stdout=PIPE, stdin=PIPE)
                    brotli_rt, _stderr = brotli_proc.communicate(brotli_dict_out)
                    assert brotli_rt == new_file

                    zlib_delta = zlib.compress(rdiff_delta)
                    zlib9_delta = zlib.compress(rdiff_delta, 9)
                    brotli_proc = subprocess.Popen([BROTLI_BIN, '-c'],
                                                   stdout=PIPE,
                                                   stdin=PIPE)
                    brotli_delta_out, _stderr = brotli_proc.communicate(rdiff_delta)
                    divans_proc = subprocess.Popen([DIVANS_BIN] + DIVANS_ARGS,
                                                   stdout=PIPE, stdin=PIPE)
                    divans_delta_out, _stderr = divans_proc.communicate(rdiff_delta)
                    ret = divans_proc.wait()
                    assert not ret
                    divans_proc = subprocess.Popen([DIVANS_BIN],
                                                   stdout=PIPE, stdin=PIPE)
                    divans_delta_rt, _stderr = divans_proc.communicate(divans_delta_out)
                    assert divans_delta_rt == rdiff_delta
                    brotli_proc = subprocess.Popen([BROTLI_BIN, '-c'],
                                                   stdout=PIPE,
                                                   stdin=PIPE)
                    brotli_raw_out, _stderr = brotli_proc.communicate(new_file)
                    zlib9_raw_out = zlib.compress(new_file, 9)
                    zlib_raw_out = zlib.compress(new_file)
                    results = {
                        'dict_divans': len(divans_dict_out),
                        'dict_brotli': len(brotli_dict_out),
                        'delta_divans': len(divans_delta_out),
                        'delta_brotli': len(brotli_delta_out),
                        'delta_zlib': len(zlib_delta),
                        'delta_zlib9': len(zlib9_delta),
                        'raw_brotli': len(brotli_raw_out),
                        'raw_zlib': len(zlib_raw_out),
                        'raw_zlib9': len(zlib9_raw_out),
                        }
                    global output_queue
                    output_queue.put(results)
                    
def compare_algorithms(old_file, new_file):
    for block_size in (256, 512, 768, 1024, 1576, 2048, 4096):
        compare_algo(old_file, new_file, block_size, 1)

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
