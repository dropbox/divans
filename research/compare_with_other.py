import os
import random as insecure_random
import subprocess
import sys
import tempfile
import threading
import traceback
import zlib
from collections import defaultdict

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

gopts = [
         ['-q9.5', '-O2', '-speed=8,8192', '-bytescore=340'],
         ['-q9.5', '-O2', '-speed=1,16384', '-bytescore=640'],
         ['-q9.5', '-O2', '-speed=128,16384', '-bytescore=340'],
         ['-q9.5', '-O2', '-speed=32,4096', '-bytescore=540'],
         ['-q9.5', '-O2', '-speed=128,256', '-bytescore=440'],
         ['-q9.5', '-O2', '-speed=8,8192', '-bytescore=140'],
         ['-q9.5', '-O2', '-speed=2,1024', '-bytescore=840'],
         ['-q9.5', '-O2', '-speed=1024,16384', '-bytescore=240'],
         ['-q9.5', '-O2', '-speed=64,16384', '-bytescore=940'],
         ['-q9.5', '-O2', '-speed=2,1024', '-bytescore=40'],
         ['-q9', '-O2', '-speed=8,8192', '-bytescore=340'],
         ['-q9', '-O2', '-speed=1,16384', '-bytescore=640'],
         ['-q9', '-O2', '-speed=128,16384', '-bytescore=340'],
         ['-q9', '-O2', '-speed=32,4096', '-bytescore=540'],
         ['-q9', '-O2', '-speed=128,256', '-bytescore=440'],
         ['-q9', '-O2', '-speed=8,8192', '-bytescore=140'],
         ['-q9', '-O2', '-speed=2,1024', '-bytescore=840'],
         ['-q9', '-O2', '-speed=1024,16384', '-bytescore=240'],
         ['-q9', '-O2', '-speed=64,16384', '-bytescore=940'],
         ['-q9', '-O2', '-speed=2,1024', '-bytescore=40'],
         ['-q9', '-mixing=2', '-O2', '-speed=8,8192', '-bytescore=340'],
         ['-q9', '-mixing=2', '-O2', '-speed=1,16384', '-bytescore=640'],
         ['-q9', '-mixing=2', '-O2', '-speed=128,16384', '-bytescore=340'],
         ['-q9', '-mixing=2', '-speed=2,1024', '-bytescore=40'],
         [],
         ['-O2'],
         ['-mixing=2'],
         ['-O2','-mixing=2'],
         ['-q8', '-O2', '-speed=8,8192', '-bytescore=340'],
         ['-q8', '-O2', '-speed=1,16384', '-bytescore=640'],
         ['-q8', '-O2', '-speed=128,16384', '-bytescore=340'],
]

lock = threading.Lock()
brotli_divans_hybrid = 0
opt_brotli_divans_hybrid = 0
brotli_total = defaultdict(lambda:0)
divans_total = 0
baseline_total = 0

def start_thread(path,
                 exe,
                 uncompressed,
                 out_array,
                 gopts,
                 index,
                 opt_args):
    def start_routine():
        try:
            compressor = subprocess.Popen(
                ['/usr/bin/nice', '-n', '15', exe, '-c'] + gopts[index] + opt_args,
                stdout=subprocess.PIPE,
                stdin=subprocess.PIPE)
            compressed, _x = compressor.communicate(uncompressed)
            cexit_code = compressor.wait()
            uncompressor = subprocess.Popen([exe],
                                            stdout=subprocess.PIPE,
                                            stdin=subprocess.PIPE)
            odat, _y = uncompressor.communicate(compressed)
            exitcode = uncompressor.wait()
            if odat != uncompressed or exitcode != 0 or cexit_code != 0:
                with lock:
                    print 'error:',path, len(odat),'!=',len(
                        uncompressed), exitcode, cexit_code,  ' '.join(
                            [exe, '-c'] + gopts[index])
                    out_array[index] = uncompressed
            else:
                out_array[index] = compressed
        except Exception:
            out_array[index] = uncompressed
            traceback.print_exc()
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
                    data = fff.read(33 * 1024 * 1024)
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

def process_file(path, data, baseline_compression, weight=1):
    global lock
    global brotli_total
    global brotli_divans_hybrid
    global opt_brotli_divans_hybrid
    global divans_total
    global printed_header
    global baseline_total
    with lock:
        if not printed_header:
            printed_header = True
            to_print = []
            print 'hdr:', gopts
    with tempfile.NamedTemporaryFile(delete=True) as tf:
        tf.write(data)
        tf.flush()
        brotli_process = {}
        brotli_process[95] = subprocess.Popen([other, '-c', tf.name],
                                              stdout=subprocess.PIPE)
        brotli_process[11] = subprocess.Popen([vanilla, '--best', '-c', tf.name],
                                                   stdout=subprocess.PIPE)
        for quality in [9,10]:
            brotli_process[quality] = subprocess.Popen(
                [vanilla, '-q', str(quality), '-c', tf.name],
                stdout=subprocess.PIPE)
        brotli_process['z'] = subprocess.Popen([zstd, '-q', '-19', tf.name,
                                                '-f', '-o', '/dev/stdout'],
                                               stderr=subprocess.PIPE,
                                               stdout=subprocess.PIPE)
        output_files = [data] * len(gopts)
        threads = []
        for index in range(len(output_files)):
            threads.append(start_thread(path,
                                        divans,
                                        data,
                                        output_files,
                                        gopts,
                                        index,
                                        []))
        for t in threads:
            t.join()
        final_len = min(min(len(op) for op in output_files),
                        len(data) + 24)
        compressed = {}
        stderr = {}
        for k, proc in brotli_process.iteritems():
            compressed[k], stderr[k] = proc.communicate()
        for k, proc in brotli_process.iteritems():
            exit_code = proc.wait()
            if exit_code != 0:
                print 'error:brotli ' + k + ':' + stderr[k]
            assert exit_code == 0
        with lock:
            divans_total += int(final_len * weight)
            for k, v in compressed.iteritems():
                brotli_total[k] += int(min(len(v), baseline_compression) * weight)
            brotli_divans_hybrid += int(min(len(compressed[95]),
                                            final_len) * weight)
            baseline_total += baseline_compression * weight
            print 'stats:', final_len, 'vs', len(
                compressed[95]), 'vsIX', len(
                    compressed[9]), 'vsX', len(
                        compressed[10]), 'vsXI', len(
                            compressed[11]), 'vsZstd',len(
                                compressed['z']), 'vsZ:',baseline_compression, \
                                'vsU', len(data)
            for best_index in range(len(output_files)):
                if len(output_files[best_index]) == final_len:
                    break
            print 'best:', gopts[best_index] if best_index < len(gopts) else 'uncompressed'
            print 'sum:', divans_total, 'vs', \
                brotli_total[95], 'vsIX', brotli_total[9], 'vsX', \
                brotli_total[10], 'vsXI', brotli_total[11], \
                'vsZ', brotli_total['z'], \
                'vs baseline:', baseline_total
            print 'args:', [len(i) for i in output_files], path
            print divans_total * 100 /float(
                brotli_total[95]), '% hybrid:', brotli_divans_hybrid *100/float(
                    brotli_total[95]),'% vsZ ', \
                    divans_total*100/float(
                        baseline_total), '% vs brotliIX ', \
                        divans_total*100/float(
                            brotli_total[9]), '% vs brotliX ', \
                        divans_total*100/float(
                            brotli_total[10]), '% vs brotliXI ', \
                        divans_total*100/float(
                            brotli_total[11]), '% vs zstd ', \
                            divans_total*100/float(
                            brotli_total['z'])
            sys.stdout.flush()

if __name__ == "__main__":
    main()
