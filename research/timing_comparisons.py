import os
import json
import random as insecure_random
import subprocess
import sys
import tempfile
import time
import threading
import traceback

import zlib
from collections import (defaultdict, namedtuple)


CompressCommand = namedtuple('CompressCommand',
                             ['name',
                              'arglist',
                             ])
cutoff_to_probe_files = 512 * 1024
walk_dir = "/"
divans = "/bin/false"
other = "/bin/false"
vanilla = "/bin/false"
zstd = "/bin/false"
if __name__ == '__main__':
    walk_dir = sys.argv[1]
    divans = sys.argv[2]
    other = sys.argv[3]
    vanilla = sys.argv[4]
    if len(sys.argv) > 5:
        zstd = sys.argv[5]
    else:
        zstd = os.path.dirname(vanilla) + "/zstd"

# speeds defined named in divans
# speeds = ["0,32", "1,32", "1,128", "1,16384",
#          "2,1024", "4,1024", "8,8192", "16,48",
#          "16,8192", "32,4096", "64,16384", "128,256",
#          "128,16384", "512,16384", "1664,16384"]

brotlistride = '-w22'

gopts = []
gopts.append([
    ['-q9', '-s', '-cm', '-mixing=2', brotlistride, '-speed=8,8192', '-bytescore=340'],
    ['-q9', '-s', '-cm', '-mixing=2', brotlistride, '-speed=1,16384', '-bytescore=640'],
    ['-q9', '-s', '-cm', '-mixing=2', brotlistride, '-speed=8,8192', '-bytescore=140'],
    ['-q9', '-s', '-cm', '-mixing=2', brotlistride, '-speed=2,1024', '-bytescore=40'],
    ['-q9', '-s', '-cm', '-mixing=2', brotlistride, '-speed=2,1024', '-bytescore=840'],
])
gopts.append([
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=8,8192', '-bytescore=340'],
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=1,16384', '-bytescore=640'],
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=8,8192', '-bytescore=140'],
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=2,1024', '-bytescore=40'],
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=2,1024', '-bytescore=840'],
])
gopts.append([
    ['-s', brotlistride, '-speed=8,8192', '-bytescore=340'],
    ['-cm', '-speed=1,16384', '-bytescore=640'],
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=8,8192', '-bytescore=140'],
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=2,1024', '-bytescore=840'],
])

gopts.append([
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=8,8192', '-speedlow=16,8192',
     '-bytescore=340'],
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=8,8192', '-speedlow=4,4096',
     '-bytescore=140'],
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=128,16384',
     '-bytescore=340'],
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=8,8192', '-speedlow=4,8192',
     '-bytescore=340'],
    ['-s', '-cm', '-mixing=2', brotlistride, '-speed=2,1024',
     '-bytescore=840'],
    ['-s', brotlistride, '-speed=8,8192', '-bytescore=340']
])

gopts.append([
    [u'-s', u'-cm', u'-mixing=2', brotlistride, u'-speed=8,8192', u'-speedlow=16,8192',
     u'-bytescore=340'],
    [u'-s', u'-cm', u'-mixing=2', brotlistride, u'-speed=2,1024',
     u'-bytescore=840'],
    [u'-s', u'-cm', u'-mixing=2', brotlistride, u'-speed=128,16384', u'-speedlow=64,16384',
     u'-bytescore=340'],
    [u'-s', u'-cm', u'-mixing=2', brotlistride, u'-speed=8,8192', u'-speedlow=4,8192',
     u'-bytescore=340']])

gopts.append([
    [u'-s', u'-cm', u'-mixing=2', '-brotlistride', u'-speed=8,8192', u'-speedlow=16,8192',
     u'-bytescore=340'],
    [u'-s', u'-cm', u'-mixing=2', '-brotlistride', u'-speed=1,16384',
     u'-bytescore=840'],
    [u'-s', u'-cm', u'-mixing=2', '-brotlistride', u'-speed=128,16384', u'-speedlow=64,16384',
     u'-bytescore=340'],
    [u'-s', u'-cm', u'-mixing=2', '-brotlistride', u'-speed=8,8192', u'-speedlow=4,8192',
     u'-bytescore=340']])

lock = threading.Lock()
brotli_divans_hybrid = 0
opt_brotli_divans_hybrid = 0
brotli_total = defaultdict(lambda:0)
divans_total = 0
baseline_total = 0
def get_best_size(path, data, output_files, output_times, opts):
    threads = []
    with tempfile.NamedTemporaryFile(dir='/dev/shm', delete=True) as temp_file:
        temp_file.write(data)
        temp_file.flush()
        for index in range(len(opts)):
            threads.append(start_thread(path,
                                        divans,
                                        data,
                                        temp_file.name,
                                        output_files,
                                        output_times,
                                        opts,
                                        index))
        for t in threads:
            t.join()

def start_thread(path,
                 exe,
                 uncompressed,
                 uncompressed_file_name,
                 out_array,
                 time_array,
                 gopts,
                 index):
    def start_routine():
        start =time.time()
        try:
            compressed = subprocess.check_output(
                [exe, '-c', uncompressed_file_name] + gopts[index]
                )
            out_array[index] = compressed
        except Exception:
            out_array[index] = uncompressed
            traceback.print_exc()
        time_array[index] = time.time() - start
    t = threading.Thread(target=start_routine)
    t.start()
    return t

def main():
    for root, subdirs, files in os.walk(walk_dir):
        for filename in files:
            path = os.path.join(root, filename)
            try:
                metadata = os.stat(path)
                if metadata.st_size < 32 * 1024:
                    continue
            except Exception:
                continue
            try:
                with open(path) as fff:
                    if metadata.st_size > 4096 * 1024:
                        fff.seek(insecure_random.randrange(0,
                                                  metadata.st_size - 4194304))
                    data = fff.read(4096 * 1024)
            except Exception:
                continue
            if filename.lower().endswith('.jpg'):
                continue
            if filename.lower().endswith('.jpeg'):
                continue
            if len(data) < 32 * 1024:
                continue
            process_file(path, data, len(zlib.compress(data)),
                         metadata.st_size/float(len(data)))
printed_header = False

def process_file(path, data, baseline_compression, weight=1):
    global lock
    global brotli_total
    global brotli_divans_hybrid
    global opt_brotli_divans_hybrid
    global divans_total
    global printed_header
    global baseline_total
    uncompressed_proxy = ['\x00'] * baseline_compression
    compressed = {}
    stderr = {}
    brotli_process = {}
    brotli_timing = {}
    divans_timing = [0] * len(gopts)
    divans_prescient_timing = [0] * len(gopts)
    divans_dtiming = [0] * len(gopts)
    divans_sizes = [baseline_compression] * len(gopts)
    divans_best_index = []
    for q_arg_list in (
            CompressCommand(name=95, arglist=[other, '-c', '/dev/stdin']),
            CompressCommand(name=11, arglist=[vanilla, '--best', '-c', '/dev/stdin']),
            CompressCommand(name=9, arglist=[vanilla, '-q', str(9), '-c', '/dev/stdin']),
            CompressCommand(name=10, arglist=[vanilla, '-q', str(10), '-c', '/dev/stdin']),
            CompressCommand(name='z', arglist=[zstd, '-q', '-19', '-o', '/dev/stdout']),
            CompressCommand(name='z22', arglist=[zstd, '-q', '-22', '-o', '/dev/stdout'])):
        n = q_arg_list.name
        start = time.time()
        brotli_process[n] = subprocess.Popen(q_arg_list.arglist,
                                                           stdin=subprocess.PIPE,
                                                           stdout=subprocess.PIPE)
        compressed[n], stderr[n] = brotli_process[n].communicate(data)
        brotli_timing[n] = time.time() - start
    raw_output_data = []
    for (opt_index, opts) in enumerate(gopts):
        output_files = ['']* len(opts)
        output_times = [0]*len(opts)
        start = time.time()
        if len(data) > cutoff_to_probe_files:
            xoff = insecure_random.randrange(0, len(data) - len(data) // 8)
            get_best_size(path, data[xoff:xoff + len(data) // 8], output_files, output_times, opts)
            min_out = min([len(of) for of in output_files])
            for index in range(len(output_files)):
                if output_files[index] == min_out:
                    break
            subp = subprocess.Popen(
                [divans, '-c'] + opts[index],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                )
            divans_compressed, _err = subp.communicate(data)
            for unc_index in range(len(output_files)):
                output_files[unc_index] = uncompressed_proxy
            output_files[index] = divans_compressed
        else:
            get_best_size(path, data, output_files, output_times, opts)

        min_item = min(min(len(item) for item in output_files),
                       baseline_compression)
        for index in range(len(output_files)):
            min_time = output_times[index]
            if output_files[index] == min_item:
                break
        if index < len(output_files):
            try:
                dec_time = time.time()
                decompressor = subprocess.Popen(
                    [divans],
                    stdout=subprocess.PIPE,
                    stdin=subprocess.PIPE)
                uncompressed, _x = decompressor.communicate(output_files[index])
                uncexit_code = decompressor.wait()
                if uncexit_code != 0 or uncompressed != data:
                    output_files = ['0' * baseline_compression] * len(opts)
                    sys.stderr.write("File " + path + "failed to roundtrip w/" + str(opts) + "\n")
                    min_item = baseline_compression
                else:
                    divans_sizes[opt_index] = len(output_files[index])
            except Exception:
                output_files = ['0' * baseline_compression] * len(opts)
                sys.stderr.write("Exception with " +path + ":" + str(opts) + "\n")
                traceback.print_exc()
                min_item = uncompressed
        divans_timing[opt_index] = time.time() - start
        divans_prescient_timing[opt_index] = min_time
        divans_dtiming[opt_index] = divans_timing[opt_index] - (dec_time - start)
        raw_output_data.append([(len(fil), ctime, divans_dtiming[opt_index]) for (
            fil, ctime) in zip(output_files, output_times)])
    with lock:
        result_map = {'~path':path, '~raw':len(data), '~':raw_output_data}
        result_map['zlib'] = (baseline_compression, 0.01, 0.01)
        for (key, val) in brotli_timing.iteritems():
            result_map[key] = (len(compressed[key]),val,0.01)
        for index in range(len(gopts)):
            result_map['divans' + str(index)] = (divans_sizes[index],
                                                 divans_timing[index],
                                                 divans_dtiming[index])
            result_map['prevans' + str(index)] = (divans_sizes[index],
                                                 divans_prescient_timing[index],
                                                 divans_dtiming[index])
        sys.stdout.write(json.dumps(result_map, sort_keys=True))
        sys.stdout.write('\n')
        sys.stdout.flush()

if __name__ == "__main__":
    main()
