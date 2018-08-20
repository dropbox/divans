import os
import random as insecure_random
import subprocess
import sys
import tempfile
import threading
import time
import traceback
import zlib
from itertools import chain
from collections import defaultdict, namedtuple
from stat import S_ISDIR, S_ISREG
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
xz = '/bin/false'
zipper = '/bin/false'
if __name__ == '__main__':
    walk_dir = sys.argv[1]
    divans = sys.argv[2]
    other = sys.argv[3]
    vanilla = sys.argv[4]
    if len(sys.argv) > 5:
        zstd = sys.argv[5]
    else:
        zstd = os.path.dirname(vanilla) + "/zstd"
    if len(sys.argv) > 6:
        xz = sys.argv[6]
    else:
        xz = os.path.dirname(vanilla) + "/xz"
    if len(sys.argv) > 7:
        zipper = sys.argv[7]
    else:
        xz = os.path.dirname(vanilla) + "/zipper"

# speeds defined named in divans
# speeds = ["0,32", "1,32", "1,128", "1,16384",
#          "2,1024", "4,1024", "8,8192", "16,48",
#          "16,8192", "32,4096", "64,16384", "128,256",
#          "128,16384", "512,16384", "1664,16384"]

brotlistride = '-w22'

gopts = []

gopts.append([ #0
    ['-O2', '-q9.5', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048', '-bytescore=40'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-sign', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=32,4096', '-bytescore=840'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
])

gopts.append([ #1
    ['-O2', '-q9.5', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048', '-bytescore=40'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-sign', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=32,4096', '-bytescore=840'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
])

gopts.append([ #2
    ['-O2', '-q9.5', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048', '-bytescore=40'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-sign', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=32,4096', '-bytescore=840'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
])

gopts.append([ #3
    ['-O2', '-q9.5', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048', '-bytescore=40'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-sign', '-speed=16,8192'],
    ['-O2', '-q9.5', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=32,4096', '-bytescore=840'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
])

gopts.append([ #4
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048', '-bytescore=40'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q10', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-sign', '-speed=16,8192'],
    ['-O2', '-q11', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=32,4096', '-bytescore=840'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
])

gopts.append([ #5
    ['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048', '-bytescore=40'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
     '-sign', '-speed=32,4096'],
    ['-O2', '-q10', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-sign', '-speed=16,8192'],
    ['-O2', '-q11', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=32,4096', '-bytescore=840'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
     '-lsb', '-speed=2,1024'],
])

gopts.append([ #6
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340'],
])

gopts.append([ #7
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340'],
])

gopts.append([ #8
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=840'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=840'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140'],
    ['-O2', '-q8', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=540'],
    ['-O2', '-q7', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=540'],
])

gopts.append([ #9
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340'],
    ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=840'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=840'],
    ['-O2', '-q9', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140'],
    ['-O2', '-q8', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=540'],
    ['-O2', '-q7', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=540'],
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


def get_best_ir_size(path, all_irs, brotli_ir, output_files, output_times):
    threads = []
    zipped_irs = []
    with tempfile.NamedTemporaryFile(dir='/dev/shm', delete=True) as ir1:
        ir1.write(brotli_ir)
        ir1.flush()
        for ir in all_irs:
            with tempfile.NamedTemporaryFile(dir='/dev/shm', delete=True) as ir0:
                ir0.write(ir)
                ir0.flush()
                zipped_irs.append(subprocess.check_output([zipper, ir0.name, ir1.name]))
    for index in range(len(all_irs)):
        args = ['-i']
        threads.append(start_ir_thread(path,
                                        divans,
                                       all_irs[index],
                                       output_files,
                                       output_times,
                                       args + (['-priordepth=65535'] if index == 0 else []),
                                       index))
        threads.append(start_ir_thread(path,
                                        divans,
                                       zipped_irs[index],
                                       output_files,
                                       output_times,
                                       args + (['-priordepth=65535'] if index == 0 else []),
                                       index + len(all_irs)))
        threads.append(start_ir_thread(path,
                                        divans,
                                       all_irs[index],
                                       output_files,
                                       output_times,
                                       args + ['-priordepth=65535'],
                                       index + 2 * len(all_irs)))
    for t in threads:
        t.join()
    #with open('/home/danielrh/dev/rust-divans/sfc3.dv') as fff:
    #    output_files[2]=fff.read()
    #print "BEST IR",[len(f) for f in output_files]
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

def start_ir_thread(path,
                 exe,
                 uncompressed,
                 out_array,
                 time_array,
                 gopts,
                 index):
    def start_routine():
        start =time.time()
        try:
            compressed_proc = subprocess.Popen(
                [exe] + gopts,
                stdout=subprocess.PIPE,
                stdin=subprocess.PIPE
                )
            compressed, _stderr = compressed_proc.communicate(uncompressed)
            out_array[index] = compressed
        except Exception:
            out_array[index] = uncompressed
            traceback.print_exc()
        time_array[index] = time.time() - start
    t = threading.Thread(target=start_routine)
    t.start()
    return t

def main():
    file_list_root = ""
    stdin_files = []
    for file in sys.stdin:
        stdin_files.append(file.strip())
    insecure_random.shuffle(stdin_files)
    for root, subdirs, files in chain([(file_list_root, [], stdin_files)], os.walk(walk_dir)):
        for filename in files:
            path = os.path.join(root, filename)
            sys.stderr.write('working '+path+'\n')
            sys.stderr.flush()
            try:
                metadata = os.stat(path)
                if S_ISDIR(metadata.st_mode):
                    continue
                if not S_ISREG(metadata.st_mode):
                    continue
                if metadata.st_size <  1024:
                    continue
            except Exception:
                traceback.print_exc()
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
            if len(data) < 1024:
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

def determine_brotli_trunc_best(other, data, quality, name, proc_map, compress_map, timing_map, frac):
    index = 0
    best_index = 0
    trials =('40','140','240','340','440','540','640','840');
    trial_proc_map = {}
    for bytescore in trials:
        trial_proc_map[bytescore] = subprocess.Popen(
            [other, quality, '-c', '/dev/stdin', '-bytescore=' + bytescore],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE)
    time.sleep(1)
    trunc_data = data
    if len(data) > 16384:
        trunc_data = data[len(data)//2 - len(data)//(frac * 2):len(data)//2 + (len(data) + 7)//(frac*2)]
    start = time.time()
    for bytescore in trials:
        pcompressed, pstderr = trial_proc_map[bytescore].communicate(trunc_data)
        if name not in compress_map or len(pcompressed) < len(compress_map[name]):
            compress_map[name] = pcompressed
            best_index = index
        index += 1
    proc_map[name] = subprocess.Popen(
        [other, quality, '-c', '/dev/stdin', '-bytescore=' + trials[best_index]],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE)
    pcompressed, pstderr = proc_map[name].communicate(data)
    compress_map[name] = pcompressed
    timing_map[name] = time.time() - start

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
    brotli_stderr = {}
    brotli_process = {}
    brotli_timing = {}
    brotli_dtiming = {}
    divans_timing = [0] * len(gopts)
    divans_dtiming = [0] * len(gopts)
    divans_stiming = [0] * len(gopts)
    divans_sizes = [baseline_compression] * len(gopts)
    for q_arg_list in (
            CompressCommand(name='b95', arglist=[other, '-q9.5', '-c', '/dev/stdin'], darglist=[other]),
            CompressCommand(name='b94', arglist=[other, '-q9.5', '-c', '-i', '/dev/stdin'], darglist=[other]),
            CompressCommand(name='b11', arglist=[
                other, '-q11', '-c', '-i', '/dev/stdin'], darglist=[other]),
            CompressCommand(name='b9', arglist=[
                vanilla, '-q', str(9), '-c', '/dev/stdin'], darglist=[other]),
            CompressCommand(name='b10', arglist=[
                vanilla, '-q', str(10), '-c', '/dev/stdin'], darglist=[vanilla, '-d']),
            CompressCommand(name='bz', arglist=[
                '/bin/bzip2', '-9', '-q', '-z'], darglist=['/bin/bzip2', '-d','-q']),
            CompressCommand(name='lzma', arglist=[
                xz, '-9', '-q', '-z'], darglist=[xz, '-d','-q']),
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
                                             stderr=subprocess.PIPE,
                                             stdout=subprocess.PIPE)
        compressed[n], brotli_stderr[n] = brotli_process[n].communicate(data)
        brotli_timing[n] = time.time() - start
        if n == 'z19' or n == 'z21':
            x = subprocess.Popen(q_arg_list.darglist,
                                 stdin=subprocess.PIPE,
                                 stderr=subprocess.PIPE,
                                 stdout=subprocess.PIPE)
            _out, brotli_stderr[n] = x.communicate(compressed[n])
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
            compressed['b91'] = uncompressed_proxy
            start = time.time()
            for bytescore in ('40','140','240','340','440','540','640','840'):
                start = time.time()
                brotli_process['b91'] = subprocess.Popen(
                    [other, '-q9', '-c', '/dev/stdin', '-bytescore=' + bytescore],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE)
                pcompressed, pstderr = brotli_process['b91'].communicate(data)
                if len(pcompressed) < len(compressed['b91']):
                    compressed['b91'] = pcompressed
                    brotli_timing['b91'] = time.time() - start
            brotli_timing['b91'] = time.time() - start
            determine_brotli_trunc_best(other, data, '-q8', 'b81e', brotli_process, compressed, brotli_timing, 4)
            determine_brotli_trunc_best(other, data, '-q9', 'b91e', brotli_process, compressed, brotli_timing, 4)
            determine_brotli_trunc_best(other, data, '-q9.5', 'b95e', brotli_process, compressed, brotli_timing, 4)

            determine_brotli_trunc_best(other, data, '-q8', 'b81t', brotli_process, compressed, brotli_timing, 8)
            determine_brotli_trunc_best(other, data, '-q9', 'b91t', brotli_process, compressed, brotli_timing, 8)
            determine_brotli_trunc_best(other, data, '-q9.5', 'b95t', brotli_process, compressed, brotli_timing, 8)

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
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
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
            brotli_dtiming['b91t'] = dend - dstart
            brotli_dtiming['b81t'] = dend - dstart
            brotli_dtiming['b95t'] = dend - dstart
            brotli_dtiming['b91e'] = dend - dstart
            brotli_dtiming['b81e'] = dend - dstart
            brotli_dtiming['b95e'] = dend - dstart
            brotli_dtiming['b91'] = dend - dstart
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
        use_old = False
        get_best_size(path, data[:len(data)//2], firsthalf_output_files, output_times, opts, use_old)
        get_best_size(path,data[len(data)//2:], secondhalf_output_files, output_times, opts, use_old)
        half_output_files = [fh + sh for (fh, sh)
                             in zip(firsthalf_output_files, secondhalf_output_files)]
        get_best_size(path, data_sixteenth, sixteenth_output_files, output_times, opts, use_old)
        get_best_size(path, data_fourth, fourth_output_files, output_times, opts, use_old)
        get_best_size(path, data_eighth, eighth_output_files, output_times, opts, use_old)
        start = time.time()
        if opt_index < 2: #try the whole file here
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
        if (opt_index & 1) != 0:
            ir_output_files = ['','','','', '', '']
            ir_output_times = [0,0,0,0, 0, 0]
            assert brotli_stderr['lzma']
            assert brotli_stderr['z19']
            get_best_ir_size(path, [brotli_stderr['lzma'], brotli_stderr['z19']], brotli_stderr['b11'], ir_output_files, ir_output_times)
            output_files.extend(ir_output_files)
            output_times.extend(ir_output_times)
            min_index = min(range(len(output_files)), key=lambda i:
                            len(output_files[i]) if i >= len(opts) or i == min_index else 100*len(output_files[i]))
        
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
                        opts[min_index] if min_index < len(opts) else 'mashup'+str(min_index - len(opts))).replace("',","'") + "\n")
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
            provisional_dtiming = baseline_compression / 1000000000.0 + .001
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
                                 divans_dtiming[min(opt_index, len(divans_dtiming) - 1)],
        ) for (
                       fil, ctime) in zip(output_files,
                                                              output_times)])
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
