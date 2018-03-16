import os
import random as insecure_random
import subprocess
import sys
import tempfile
import threading
import traceback
import zlib

walk_dir = "/"
divans = "/bin/false"
other = "/bin/false"
vanilla = "/bin/false"
if __name__ == '__main__':
    walk_dir = sys.argv[1]
    divans = sys.argv[2]
    other = sys.argv[3]
    vanilla = sys.argv[4]

speeds = ["0,32", "1,32", "1,128", "1,16384",
          "2,1024", "4,1024", "8,8192", "16,48",
          "16,8192", "32,4096", "64,16384", "128,256",
          "128,16384", "512,16384", "1664,16384"]

gopts = [['-s', '-cm', '-mixing=2', '-brotlistride', '-speed=8,8192', '-bytescore=340'],
         ['-s', '-cm', '-mixing=2', '-brotlistride', '-speed=1,16384', '-bytescore=640'],
         ['-s', '-cm', '-mixing=2', '-brotlistride', '-speed=128,16384', '-bytescore=340'],
         ['-s', '-cm', '-mixing=2', '-brotlistride', '-speed=32,4096', '-bytescore=540'],
         ['-s', '-cm', '-mixing=2', '-brotlistride', '-speed=128,256', '-bytescore=440'],
         ['-s', '-cm', '-mixing=2', '-brotlistride', '-speed=8,8192', '-bytescore=140'],
         ['-s', '-cm', '-mixing=2', '-brotlistride', '-speed=2,1024', '-bytescore=840'],
         ['-s', '-cm', '-mixing=2', '-brotlistride', '-speed=1024,16384', '-bytescore=240'],
         ['-s', '-cm', '-mixing=2', '-brotlistride', '-speed=64,16384', '-bytescore=940'],
         ['-s', '-cm', '-mixing=2', '-brotlistride', '-speed=2,1024', '-bytescore=40'],

         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=8,8192', '-speedlow=16,8192', '-bytescore=340'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=1,16384', '-speedlow=2,16384', '-bytescore=640'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=128,16384', '-speedlow=256,16384', '-bytescore=340'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=32,4096', '-speedlow=64,8192', '-bytescore=540'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=128,256', '-speedlow=512,16384', '-bytescore=440'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=8,8192', '-speedlow=16,16384', '-bytescore=140'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=2,1024', '-speedlow=4,2048', '-bytescore=840'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=1024,16384', '-speedlow=2048,16384', '-bytescore=240'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=64,16384', '-speedlow=128,16384', '-bytescore=940'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=2,1024', '-speedlow=4,4096', '-bytescore=40'],
                  
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=8,8192', '-speedlow=4,8192', '-bytescore=340'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=1,16384', '-speedlow=1,128', '-bytescore=640'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=128,16384', '-speedlow=64,16384', '-bytescore=340'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=32,4096', '-speedlow=16,2048', '-bytescore=540'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=128,256', '-speedlow=64,256', '-bytescore=440'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=8,8192', '-speedlow=4,4096', '-bytescore=140'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=2,1024', '-speedlow=1,512', '-bytescore=840'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=1024,16384', '-speedlow=512,16384', '-bytescore=240'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=64,16384', '-speedlow=32,16384', '-bytescore=940'],
         ['-s', '-cm', '-mixing=2', '-brotlistride',
          '-speed=2,1024', '-speedlow=1,4096', '-bytescore=40'],

         ['-s', '-brotlistride', '-speed=8,8192', '-bytescore=340'],
         ['-s', '-brotlistride', '-speed=1,16384', '-bytescore=640'],
         ['-s', '-brotlistride', '-speed=128,16384', '-bytescore=340'],
         ['-s', '-brotlistride', '-speed=32,4096', '-bytescore=540'],
         ['-s', '-brotlistride', '-speed=128,256', '-bytescore=440'],
         ['-s', '-brotlistride', '-speed=8,8192', '-bytescore=140'],
         ['-s', '-brotlistride', '-speed=2,1024', '-bytescore=840'],
         ['-s', '-brotlistride', '-speed=1024,16384', '-bytescore=240'],
         ['-s', '-brotlistride', '-speed=64,16384', '-bytescore=940'],
         ['-s', '-brotlistride', '-speed=2,1024', '-bytescore=40'],

         ['-cm', '-findspeed', '-bytescore=340'],
         ['-cm', '-speed=8,8192', '-bytescore=340'],
         ['-cm', '-speed=1,16384', '-bytescore=640'],
         ['-cm', '-speed=128,16384', '-bytescore=340'],
         ['-cm', '-speed=32,4096', '-bytescore=540'],
         ['-cm', '-speed=128,256', '-bytescore=440'],
         ['-cm', '-speed=8,8192', '-bytescore=140'],
         ['-cm', '-speed=2,1024', '-bytescore=840'],
         ['-cm', '-speed=1024,16384', '-bytescore=240'],
         ['-cm', '-speed=64,16384', '-bytescore=940'],
         ['-cm', '-speed=2,1024', '-bytescore=40'],
]

lock = threading.Lock()
brotli_divans_hybrid = 0
opt_brotli_divans_hybrid = 0
brotli_total = 0
brotli9_total = 0
brotli10_total = 0
brotli11_total = 0
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
    global brotli9_total
    global brotli10_total
    global brotli11_total
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
        b11 = subprocess.Popen([vanilla, '--best', '-c', tf.name],
                                stdout=subprocess.PIPE)
        b = subprocess.Popen([other, '-c', tf.name],
                                stdout=subprocess.PIPE)
        b9 = subprocess.Popen([vanilla, '-q', '9', '-c', tf.name],
                              stdout=subprocess.PIPE)
        b10 = subprocess.Popen([vanilla, '-q', '10', '-c', tf.name],
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
        compressed, _ok = b.communicate()
        compressed9, _ok = b9.communicate()
        compressed10, _ok = b10.communicate()
        compressed11, _ok = b11.communicate()
        exit_code = b.wait()
        if exit_code != 0:
            print 'error:brotli 0'
        assert exit_code == 0
        exit_code = b11.wait()
        if exit_code != 0:
            print 'error:brotli 11'
        exit_code = b10.wait()
        if exit_code != 0:
            print 'error:brotli 10'
        exit_code = b9.wait()
        if exit_code != 0:
            print 'error:brotli 9'
        assert exit_code == 0
        with lock:
            divans_total += int(final_len * weight)
            brotli_total += int(len(compressed) * weight)
            brotli9_total += int(len(compressed9) * weight)
            brotli10_total += int(len(compressed10) * weight)
            brotli11_total += int(len(compressed11) * weight)
            brotli_divans_hybrid += int(min(len(compressed),
                                            final_len) * weight)
            baseline_total += baseline_compression * weight
            print 'stats:', final_len, 'vs', len(
                compressed), 'vsIX', len(
                    compressed9), 'vsX', len(
                        compressed10), 'vsXI', len(
                            compressed11), 'vs baseline:',baseline_compression
            for best_index in range(len(output_files)):
                if len(output_files[best_index]) == final_len:
                    break
            print 'best:', gopts[best_index] if best_index < len(gopts) else 'uncompressed'
            print 'sum:', divans_total, 'vs', \
                brotli_total, 'vsIX', brotli9_total, 'vsX', \
                brotli10_total, 'vsXI', brotli11_total, \
                'vs baseline:', baseline_total
            print 'args:', [len(i) for i in output_files], path
            print divans_total * 100 /float(
                brotli_total), '% hybrid:', brotli_divans_hybrid *100/float(
                    brotli_total),'% vs baseline ', \
                    divans_total*100/float(
                        baseline_total), '% vs brotliIX ', \
                        divans_total*100/float(
                        brotli9_total), '% vs brotliX ', \
                        divans_total*100/float(
                        brotli10_total), '% vs brotliXI ', \
                        divans_total*100/float(
                        brotli11_total)
            sys.stdout.flush()

if __name__ == "__main__":
    main()
