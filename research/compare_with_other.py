import os
import random as insecure_random
import subprocess
import sys
import tempfile
import threading
import tuime
import traceback
import zlib

from collections import defaultdict, namedtuple

import json
CompressCommand = namedtuple('CompressCommand',
                             ['name',
                              'arglist',
                              'darglist',
                             ])
cutoff_to_probe_files = 384 * 1024
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

gopts.append([ #0
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-O2', '-q11', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior',
     '-speed=1,16384'],
    ['-O2', '-q10', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-lsb', '-speed=16,8192'],
])

gopts.append([ #1
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q10', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-sign', '-speed=16,8192'],
    ['-O2', '-q11', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
])

gopts.append([ #2
    ['-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
    ['-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-q11', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
    ['-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
    ['-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior',
     '-speed=1,16384'],
    ['-q10', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-lsb', '-speed=16,8192'],
])

gopts.append([ #3
    ['-O2', '-q9', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-O2', '-q9', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
    ['-O2', '-q9', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior',
     '-speed=1,16384'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40', '-lsb',
     '-speed=16,8192'],
    ['-O2', '-q8', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior',
     '-speed=1,16384'],
    ['-O2', '-q7', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40', '-lsb',
     '-speed=16,8192'],
])

gopts.append([ #4
    ['-O2', '-q10', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-O2', '-q10', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
])

#fast and possibly good enough
gopts.append([ #5
 ['-O2', '-q11', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-speed=8,8192'],
])


gopts.append([ #6
    ['-O2', '-q9', '-w22', '-lsb', '-lgwin22', '-mixing=2', '-speed=2,2048', '-bytescore=540'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=2', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=2', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-O2', '-q9', '-w22', '-lgwin18', '-mixing=2', '-speed=16,8192', '-bytescore=880'],
    ['-O2', '-q9', '-w22', '-lgwin18', '-mixing=2', '-speed=16,8192', '-bytescore=340'],
])


gopts.append([ #7
    ['-O2', '-q9.5', '-w22', '-lsb', '-lgwin22', '-mixing=2', '-speed=2,2048', '-bytescore=540'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=2', '-bytescore=140', '-speed=32,4096'],
])

gopts.append([ #8
    ['-O2', '-q9.5', '-w22', '-lsb', '-lgwin22', '-mixing=2', '-speed=2,2048', '-bytescore=540'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=2', '-bytescore=140', '-speed=32,4096', '-sign'],
])

gopts.append([ #9
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
])


gopts.append([ #10
    ['-O2', '-q9', '-w22', '-defaultprior', '-lsb', '-lgwin22', '-mixing=2', '-speed=2,2048', '-bytescore=540'],
    ['-O2', '-q9', '-w22', '-defaultprior', '-lgwin22', '-mixing=2', '-bytescore=140',
     '-sign', '-speed=32,4096'],
])


gopts.append([ #11
    ['-O2', '-q9.5', '-w22', '-defaultprior', '-lsb', '-lgwin22', '-mixing=2', '-speed=2,2048', '-bytescore=540'],
    ['-O2', '-q9.5', '-w22', '-defaultprior', '-lgwin22', '-mixing=2', '-bytescore=140', '-speed=32,4096', '-sign'],
])

gopts.append([ #12
    ['-O2', '-q9.5', '-w22', '-defaultprior', '-lgwin22', '-mixing=2', '-bytescore=340'],
])

gopts.append([ #13
    ['-O2', '-q9.5', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-speed=2,2048', '-bytescore=540'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-bytescore=140', '-speed=32,4096'],
])

gopts.append([ #14
    ['-O2', '-q9.5', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-speed=2,2048', '-bytescore=840'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-bytescore=140', '-speed=32,4096'],
])


gopts.append([ #15
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-speed=2,2048', '-bytescore=840'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-bytescore=340'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-bytescore=140', '-speed=32,4096'],
])

gopts.append([ #16
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-O2', '-q11', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior',
     '-speed=1,16384'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40', '-lsb',
     '-speed=16,8192'],
])

gopts.append([ #17
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-O2', '-q11', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
])

gopts.append([ #18
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=2', '-findprior', '-speed=2,2048'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=2', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=2', '-findprior', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-O2', '-q11', '-w22', '-lgwin18', '-mixing=2', '-findprior', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=2', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=2', '-findprior',
     '-speed=1,16384'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=2', '-findprior', '-bytescore=40', '-lsb',
     '-speed=16,8192'],
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=2', '-findprior',
     '-speed=1,16384'],
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=2', '-defaultprior', '-speed=1,16384'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=2', '-findprior', '-bytescore=40', '-lsb',
     '-speed=16,8192'],
])

gopts.append([ #19
    ['-O2', '-q9', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-O2', '-q9', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
    ['-O2', '-q9', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior',
     '-speed=1,16384'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40', '-lsb',
     '-speed=16,8192'],
    ['-O2', '-q9', '-w22', '-lsb', '-lgwin22', '-mixing=2', '-findprior',
     '-speed=1,16384'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=2', '-findprior', '-bytescore=40', '-lsb',
     '-speed=16,8192'],
])



gopts.append([ #20
    ['-O2', '-q10', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-O2', '-q10', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
])

gopts.append([#21
    ['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=140'],
    ['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=840', "-speed=16,8192"],
    ])

gopts.append([#22
    ['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=140'],
    ['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ['-q8', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=840', "-speed=16,8192"],
    ])

gopts.append([#23
    ['-q9.5', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=140'],
    ['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ['-q8', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=840', "-speed=16,8192"],
    ])

gopts.append([#24
    ['-q10', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=140'],
    ['-q9.5', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=140'],
    ['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ['-q8', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=840', "-speed=16,8192"],
    ])

gopts.append([#25
    ['-q9', '-defaultprior', '-w22', '-nocm', '-lgwin22', '-mixing=0', '-bytescore=140'],
    ['-q9', '-defaultprior', '-w22', '-nocm', '-lgwin22', '-mixing=0', '-bytescore=340'],
     ['-q8', '-defaultprior', '-w22', '-nocm', '-lgwin22', '-mixing=0', '-bytescore=840', "-speed=16,8192"],
    ])

gopts.append([#26
    ['-q9', '-defaultprior', '-cm', '-nostride', '-w22', '-lgwin22', '-mixing=1', '-bytescore=140'],
    ['-q9', '-defaultprior', '-cm', '-nostride', '-w22', '-lgwin22', '-mixing=1', '-bytescore=340'],
     ['-q8', '-defaultprior', '-cm', '-nostride', '-w22', '-lgwin22', '-mixing=1', '-bytescore=840', "-speed=16,8192"],
    ])

gopts.append([#27
    ['-q9.5', '-defaultprior', '-cm', '-nostride', '-w22', '-lgwin22', '-mixing=1', '-bytescore=140'],
    ['-q9', '-defaultprior', '-cm', '-nostride', '-w22', '-lgwin22', '-mixing=1', '-bytescore=340'],
    ['-q8', '-defaultprior', '-cm', '-nostride', '-w22', '-lgwin22', '-mixing=1', '-bytescore=840', "-speed=16,8192"],
    ])

gopts.append([#28
    ['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ])

gopts.append([#29
    ['-q9', '-defaultprior', '-nocm', '-w22', '-lgwin22', '-mixing=0', '-bytescore=340'],
    ])

gopts.append([#30
    ['-q9', '-defaultprior', '-w22', '-nostride', '-mixing=0', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ])

gopts.append([#31
    ['-q8', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=140'],
    ['-q8', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ['-q8', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=840'],
    ])

gopts.append([#32
    ['-q8', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ])

gopts.append([#33
    ['-q9', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ])

gopts.append([#34
    ['-q7', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ])

gopts.append([#35
    ['-q7', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=40'],
    ['-q7', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ['-q7', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=540'],
    ['-q7', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=840', '-speed=1,16384'],
    ])
gopts.append([#36
    ['-q6', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ])

gopts.append([#37
    ['-q6', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=40'],
    ['-q6', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ['-q6', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=540'],
    ['-q6', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=740'],
    ['-q6', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=840', '-speed=1,16384'],
    ])

gopts.append([#38
    ['-q5', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
    ])


gopts.append([ #LAST
    ['-s', '-cm', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
    ['-s', '-cm', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-s', '-cm', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40',
     '-sign', '-speed=16,8192'],
    ['-s', '-cm', '-q11', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
    ['-s', '-cm', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
    ['-s', '-cm', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior',
     '-speed=1,16384'],
    ['-s', '-cm', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-lsb', '-speed=16,8192'],
])

lock = threading.Lock()
brotli_divans_hybrid = 0
opt_brotli_divans_hybrid = 0
brotli_total = defaultdict(lambda:0)
divans_total = 0
baseline_total = 0
def get_best_size(path, data, output_files, output_times, opts, use_old_binary):
    threads = []
    with tempfile.NamedTemporaryFile(dir='/dev/shm', delete=True) as temp_file:
        temp_file.write(data)
        temp_file.flush()
        for index in range(len(opts)):
            threads.append(start_thread(path,
                                        divans.replace('-avx','-old') if use_old_binary else divans,
                                        data,
                                        temp_file.name,
                                        output_files,
                                        output_times,
                                        opts + ([] if use_old_binary else ['-O2']),
                                        index))
        for t in threads:
            t.join()
        #min_size = min([len(f) for f in output_files])
        #if min_size < len(output_files[-1]) * 1.003 and len(output_files[-1]) > len(data):
        #    output_files[-1] = output_files[0] # not the smallest... tied at best


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

def rand_half(data):
    new_len = len(data) // 2
    if len(data) -new_len - 1 <= 2:
        return data[:]
    start = insecure_random.randrange(0, len(data) - new_len - 1)
    return data[start:start + new_len]

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
    brotli_dtiming = {}
    divans_timing = [0] * len(gopts)
    divans_dtiming = [0] * len(gopts)
    divans_stiming = [0] * len(gopts)
    divans_sizes = [baseline_compression] * len(gopts)
    for q_arg_list in (
            CompressCommand(name='b95', arglist=[other, '-q9.5', '-c', '/dev/stdin'], darglist=[other]),
            CompressCommand(name='b94', arglist=[other, '-q9.5', '-c', '/dev/stdin'], darglist=[other]),
            CompressCommand(name='b11', arglist=[
                vanilla, '--best', '-c', '/dev/stdin'], darglist=[other]),
            CompressCommand(name='b9', arglist=[
                vanilla, '-q', str(9), '-c', '/dev/stdin'], darglist=[other]),
            CompressCommand(name='b10', arglist=[
                vanilla, '-q', str(10), '-c', '/dev/stdin'], darglist=[vanilla, '-d']),
            CompressCommand(name='bz', arglist=[
                '/bin/bzip2', '-9', '-q', '-z'], darglist=['/bin/bzip2', '-d','-q']),
            CompressCommand(name='lzma', arglist=[
                '/usr/bin/lzma', '-9', '-q', '-z'], darglist=['/usr/bin/lzma', '-d','-q']),
            CompressCommand(name='z19', arglist=[
                zstd, '-q', '-19', '-o', '/dev/stdout'],
                darglist=[zstd, '-q', '-d']),
            CompressCommand(name='z22', arglist=[
                zstd, '-q', '-22', '-o', '/dev/stdout'],
                darglist=[zstd, '-q', '-d'])):
        n = q_arg_list.name
        start = time.time()
        brotli_process[n] = subprocess.Popen(q_arg_list.arglist,
                                             stdin=subprocess.PIPE,
                                             stdout=subprocess.PIPE)
        compressed[n], stderr[n] = brotli_process[n].communicate(data)
        brotli_timing[n] = time.time() - start
        if n == 'b95':
            for bytescore in ('40','140','240','340','440','540','640','840'):
                start = time.time()
                brotli_process[n] = subprocess.Popen(
                    q_arg_list.arglist + ['-bytescore=' + bytescore],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE)
                pcompressed, pstderr = brotli_process[n].communicate(data)
                if len(pcompressed) < len(compressed[n]):
                    compressed[n] = pcompressed
                    brotli_timing[n] = time.time() - start
            compressed['b96'] = compressed[n]
            brotli_timing['b96'] = brotli_timing[n]
            start = time.time()
            brotli_process['b96'] = subprocess.Popen(
                [other, '-q11', '-c', '/dev/stdin'],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE)
            pcompressed, pstderr = brotli_process['b96'].communicate(data)
            if len(pcompressed) < len(compressed['b96']):
                compressed['b96'] = pcompressed
                brotli_timing['b96'] = time.time() - start

        if q_arg_list.darglist[0] == other:
            with tempfile.NamedTemporaryFile(dir='/dev/shm', delete=True) as brotf:
                brotf.write(compressed[n])
                brotf.flush()
                dstart = time.time()
                subprocess.check_output(q_arg_list.darglist + ['-b32', brotf.name, '/dev/null'])
                dend = time.time()
                brotli_dtiming[n] = (dend - dstart) / 32
        else:
            dprocess = subprocess.Popen(q_arg_list.darglist,
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE)
            dprocess.stdin.write(compressed[n][:2])
            dprocess.stdin.flush()
            time.sleep(.25)
            dstart=time.time()
            orig, err = dprocess.communicate(compressed[n][2:])
            dend = time.time()
            assert orig == data, "need to be able to brotli decompress"
            brotli_dtiming[n] = dend - dstart
        if n=='b95':
            brotli_dtiming['b96'] = dend - dstart
    raw_output_data = []
    data_half = rand_half(data)
    data_fourth = rand_half(data_half)
    data_eighth = rand_half(data_fourth)
    data_sixteenth = rand_half(data_eighth)
    for (opt_index, opts) in enumerate(gopts):
        output_files = ['']* len(opts)
        fourth_output_files = ['']* len(opts)
        eighth_output_files = ['']* len(opts)
        sixteenth_output_files = ['']* len(opts)
        firsthalf_output_files = ['']* len(opts)
        secondhalf_output_files = ['']* len(opts)
        output_times = [0]*len(opts)
        use_old = opt_index + 1 == len(gopts)
        get_best_size(path, data[:len(data)//2], firsthalf_output_files, output_times, opts, use_old)
        get_best_size(path,data[len(data)//2:], secondhalf_output_files, output_times, opts, use_old)
        half_output_files = [fh + sh for (fh, sh)
                             in zip(firsthalf_output_files, secondhalf_output_files)]
        get_best_size(path, data_sixteenth, sixteenth_output_files, output_times, opts, use_old)
        get_best_size(path, data_fourth, fourth_output_files, output_times, opts, use_old)
        get_best_size(path, data_eighth, eighth_output_files, output_times, opts, use_old)
        start = time.time()
        if opt_index <= 4 or opt_index + 1 == len(gopts): #try the whole file here
            get_best_size(path, data, output_files, output_times, opts, use_old)
            min_index = min(range(len(output_files)), key=lambda i:
                            len(output_files[i]) * (1.001 if '-mixing=2' in opts[i] else 1.000))
        else:
            min_index = 0
            if len(output_files) != 1:
                get_best_size(path, data_eighth, eighth_output_files, output_times, opts, use_old)
                min_index = min(range(len(eighth_output_files)), key=lambda i:
                                len(eighth_output_files[i]) );
            output_files = [output_files[min_index]]
            output_times = [0]
            xopts = [opts[min_index]]
            get_best_size(path, data, output_files, output_times, xopts, use_old)
            output_files = output_files * len(opts)
            output_times = output_times * len(opts)
        index = min_index
        check_uncompressed = False
        dec_time = time.time()
        min_item = output_files[min_index]
        if len(min_item) < baseline_compression and \
           output_files[min_index] != uncompressed_proxy:
            try:
                #print 'decoding file of lenght ', len(output_files[index]),index
                decompressor = subprocess.Popen(
                    [divans.replace('-avx', '-old') if use_old else divans],
                    stdout=subprocess.PIPE,
                    stdin=subprocess.PIPE)
                uncompressed, _x = decompressor.communicate(output_files[min_index])
                uncexit_code = decompressor.wait()
                if uncexit_code != 0 or uncompressed != data:
                    output_files = ['0' * baseline_compression] * len(opts)
                    sys.stderr.write("File " + path + "failed to roundtrip w/" + str(
                        opts[min_index]).replace("',","'") + "\n")
                    min_item = baseline_compression
                else:
                    check_uncompressed = True
                    divans_sizes[opt_index] = len(output_files[min_index])
            except Exception:
                output_files = ['0' * baseline_compression] * len(opts)
                sys.stderr.write("Exception with " +path + ":" + str(opts) + "\n")
                traceback.print_exc()
                min_item = uncompressed
            provisional_dtiming = time.time() - dec_time
        else:
            provisional_dtiming = time.time() - dec_time
        divans_timing[opt_index] = dec_time - start
        divans_dtiming[opt_index] = provisional_dtiming
        divans_stiming[opt_index] = divans_dtiming[opt_index]
        if min_index < len(output_files) and check_uncompressed:
            try:
                with tempfile.NamedTemporaryFile(dir='/dev/shm', delete=True) as divf:
                    divf.write(output_files[min_index])
                    divf.flush()
                    start = time.time()
                    subprocess.check_output([divans.replace('-avx', '-old') if use_old else divans, '-serial', '-benchmark=32', divf.name, '/dev/null'])
                    divans_stiming[opt_index] = (time.time() - start) / 32
            except Exception:
                traceback.print_exc()
            try:
                with tempfile.NamedTemporaryFile(dir='/dev/shm', delete=True) as divf:
                    divf.write(output_files[min_index])
                    divf.flush()
                    start = time.time()
                    subprocess.check_output([divans.replace('-avx', '-old') if use_old else divans, '-benchmark=32', divf.name, '/dev/null'])
                    divans_dtiming[opt_index] = (time.time() - start) / 32
            except Exception:
                traceback.print_exc()
        raw_output_data.append([(len(fil),
                                 ctime,
                                 divans_dtiming[opt_index],
                                 len(oo4),
                                 len(oo8),
                                 len(oo16),
                                 len(halved)) for (
                       fil, ctime,oo4,oo8,oo16,halved) in zip(output_files,
                                                              output_times,
                                                              fourth_output_files,
                                                              eighth_output_files,
                                                              sixteenth_output_files,
                                                              half_output_files)])
    with lock:
        result_map = {'~path':path, '~raw':len(data), '~':raw_output_data}
        zlib_start = time.time()
        zc = zlib.compress(data, 9)
        zlib_mid = time.time()
        zlib.decompress(zc)
        zlib_end = time.time()
        result_map['zlib'] = (min(len(zc), baseline_compression),
                              zlib_mid - zlib_start, zlib_end - zlib_mid, zlib_end - zlib_mid)
        for (key, val) in brotli_timing.iteritems():
            result_map[key] = (len(compressed[key]),val, brotli_dtiming[key], brotli_dtiming[key])
        for index in range(len(gopts)):
            result_map['d' + str(index)] = (divans_sizes[index],
                                                 divans_timing[index],
                                                 divans_dtiming[index],
                                            divans_stiming[index])
        sys.stdout.write(json.dumps(result_map, sort_keys=True))
        sys.stdout.write('\n')
        sys.stdout.flush()

if __name__ == "__main__":
    main()
