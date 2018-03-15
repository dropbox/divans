import os
import sys
import subprocess
import threading
import tempfile
import random
import traceback
walk_dir = "/"
divans = "/bin/false"
other = "/bin/false"
if __name__ == '__main__':
    walk_dir = sys.argv[1]
    divans = sys.argv[2]
    other = sys.argv[3]
    
speeds = ["0,32", "1,32", "1,128", "1,16384", "2,1024", "4,1024", "8,8192", "16,48", "16,8192", "32,4096", "64,16384", "128,256", "128,16384", "512,16384", "1664,16384"]
gopts = [[], [],[]]
gopts[0] = [['-cm', '-speed=' + speeds[0]],#0
         ['-cm', '-speed=' + speeds[1]],
         ['-cm', '-speed=' + speeds[2]],
         ['-cm', '-speed=' + speeds[3]],
         ['-cm', '-speed=' + speeds[4]],
         ['-cm', '-speed=' + speeds[5]],
         ['-cm', '-speed=' + speeds[6]],
         ['-cm', '-speed=' + speeds[7]],
         ['-cm', '-speed=' + speeds[8]],
         ['-cm', '-speed=' + speeds[9]],
         ['-cm', '-speed=' + speeds[10]],
         ['-cm', '-speed=' + speeds[11]],
         ['-cm', '-speed=' + speeds[12]],
         ['-cm', '-speed=' + speeds[13]],
         ['-cm', '-speed=' + speeds[14]],#14
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[0]],#20
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[1]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[2]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[3]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[4]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[5]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[6]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[7]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[8]],#28 lazy
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[9]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[10]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[11]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[12]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[13]],
         ['-s', '-cm','-mixing=2','-stride=1', '-speed=' + speeds[14]],#34
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[0]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[1]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[2]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[3]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[4]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[5]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[6]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[7]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[8]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[9]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[10]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[11]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[12]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[13]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[14]],
         ['-s','-stride=1', '-speed=MUD'],
         ['-s','-brotlistride', '-speed=MUD'],
         ['-s', '-cm','-mixing=2', '-brotlistride'],
]
gopts[1] = [
         ['-s','-stride=1', '-speed=16,8192'],
         ['-s','-brotlistride', '-speed=16,8192'],
         ['-cm', '-speed=16,8192'],
         ['-cm', '-speed=1,16384'],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[0]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[1]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[2]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[3]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[4]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[5]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[6]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[7]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[8]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[9]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[10]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[11]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[12]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[13]],
         ['-s', '-cm','-mixing=2','-brotlistride', '-speed=' + speeds[14]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[0]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[1]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[2]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[3]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[4]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[5]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[6]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[7]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[8]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[9]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[10]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[11]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[12]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[13]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,128", '-speed=' + speeds[14]],

         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[0]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[1]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[2]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[3]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[4]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[5]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[6]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[7]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[8]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[9]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[10]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[11]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[12]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[13]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1,16384", '-speed=' + speeds[14]],

         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[0]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[1]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[2]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[3]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[4]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[5]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[6]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[7]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[8]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[9]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[10]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[11]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[12]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[13]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=16,8192", '-speed=' + speeds[14]],

         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[0]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[1]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[2]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[3]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[4]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[5]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[6]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[7]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[8]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[9]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[10]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[11]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[12]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[13]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=4,1024", '-speed=' + speeds[14]],

         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[0]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[1]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[2]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[3]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[4]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[5]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[6]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[7]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[8]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[9]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[10]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[11]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[12]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[13]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=128,16384", '-speed=' + speeds[14]],

         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[0]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[1]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[2]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[3]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[4]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[5]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[6]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[7]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[8]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[9]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[10]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[11]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[12]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[13]],
         ['-s', '-cm','-mixing=2','-brotlistride', "-cmspeed=1664,16384", '-speed=' + speeds[14]],

    ]
lock = threading.Lock()
brotli_divans_hybrid = 0
opt_brotli_divans_hybrid = 0
brotli_total = 0
optimistic_divans_total = 0
pessimistic_divans_total = 0
baseline_total = 0
def start_thread(path, exe, uncompressed, ir, out_array, gopts, index, opt_args):
    def start_routine():
        try:
            compressor = subprocess.Popen([exe, '-i'] + gopts[index] + opt_args, stdout=subprocess.PIPE, stdin=subprocess.PIPE)
            compressed, _x = compressor.communicate(ir)
            cexit_code = compressor.wait()
            uncompressor = subprocess.Popen([exe],  stdout=subprocess.PIPE, stdin=subprocess.PIPE)
            odat, _y = uncompressor.communicate(compressed)
            exitcode = uncompressor.wait()
            if odat != uncompressed or exitcode != 0 or cexit_code != 0:
                with lock:
                    print 'error:',path, len(odat),'!=',len(uncompressed), exitcode, cexit_code,  ' '.join([exe, '-i'] + gopts[index])
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
                        fff.seek(random.randrange(0, metadata.st_size - 4096 * 1024))
                    data = fff.read(4096 * 1024)
            except Exception:
                continue
            if filename.lower().endswith('.jpg'):
                continue
            if filename.lower().endswith('.jpeg'):
                continue
            if len(data) < 32 * 1024:
                continue
            process_file(path, data, len(data), metadata.st_size/float(len(data)))
printed_header = False
def process_file(path, data, baseline_compression, weight=1):
    global lock
    global brotli_total
    global brotli_divans_hybrid
    global opt_brotli_divans_hybrid
    global optimistic_divans_total
    global pessimistic_divans_total
    global printed_header
    global baseline_total
    ir_variant_arg = ['-bytescore=540','-bytescore=240','-bytescore=340','-bytescore=380','-bytescore=440']
    with lock:
        if not printed_header:
            printed_header = True
            to_print = []
            for var in ir_variant_arg:
                for cmd in gopts[1]:
                    to_print.append(cmd + [var])
            print 'hdr:', to_print
            to_print = []
            for var in ir_variant_arg:
                for cmd in gopts[0]:
                    to_print.append(cmd + [var])
            print 'hopt:', to_print
    with tempfile.NamedTemporaryFile(delete=True) as tf:
        tf.write(data)
        tf.flush()
        p340 = subprocess.Popen([other, '-c', '-i', '-basicstride', ir_variant_arg[2], tf.name], stdout=subprocess.PIPE, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
        p380 = subprocess.Popen([other, '-c', '-i', '-basicstride', ir_variant_arg[3], tf.name], stdout=subprocess.PIPE, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
        p440 = subprocess.Popen([other, '-c', '-i', '-basicstride', ir_variant_arg[4], tf.name], stdout=subprocess.PIPE, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
        p240 = subprocess.Popen([other, '-c', '-i', '-basicstride', ir_variant_arg[1], tf.name], stdout=subprocess.PIPE, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
        otherp = subprocess.Popen([other, '-c', '-i', '-basicstride', ir_variant_arg[0]], stdout=subprocess.PIPE, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
        ir_variants = ['','','','','']
    
        compressed, ir_variants[0] = otherp.communicate(data)
        _ok, ir_variants[1] = p240.communicate('')
        _ok, ir_variants[2] = p340.communicate('')
        _ok, ir_variants[3] = p380.communicate('')
        _ok, ir_variants[4] = p440.communicate('')
    exit_code = otherp.wait()
    if exit_code != 0:
        print ir_variants[0]
    assert exit_code == 0
    exit_code = p240.wait()
    if exit_code != 0:
        print ir_variants[1]
    assert exit_code == 0
    exit_code = p340.wait()
    if exit_code != 0:
        print ir_variants[2]
    assert exit_code == 0
    exit_code = p380.wait()
    if exit_code != 0:
        print ir_variants[3]
    assert exit_code == 0
    exit_code = p440.wait()
    if exit_code != 0:
        print ir_variants[4]
    assert exit_code == 0
    first_output_files = []
    second_output_files = []
    usage =[[],[]]
    ir_variant_index = 0
    for ir in ir_variants:
        first_gopts = gopts[0]
        tmp_output_files = [compressed] * len(first_gopts)
        threads = []
        for index in range(15):
            threads.append(start_thread(path, divans, data, ir, tmp_output_files, first_gopts, index, []))
        for t in threads:
            t.join()
            best_opt_arg = first_gopts[0][-1].replace('-speed','-cmspeed')
            best_opt_size = len(tmp_output_files[0])
        for i in range(1,15):
            if len(tmp_output_files[i]) < best_opt_size:
                best_opt_size = len(tmp_output_files[i])
                best_opt_arg = first_gopts[i][-1].replace('-speed','-cmspeed')
        for index in range(15, len(tmp_output_files)):
            threads.append(start_thread(path, divans, data, ir, tmp_output_files, first_gopts, index, [best_opt_arg]))
        for t in threads:
            t.join()
        for index in range(len(first_gopts)):
            best_add_arg = []
            if index >= 15:
                best_add_arg = [best_opt_arg]
            usage[0].append(first_gopts[index] + best_add_arg + [ir_variant_arg[ir_variant_index]])
        first_output_files += tmp_output_files
        second_gopts = gopts[1]
        tmp_output_files = [''] * len(second_gopts)
        for index in range(len(tmp_output_files)):
            threads.append(start_thread(path, divans, data, ir, tmp_output_files, second_gopts, index, [best_opt_arg]))
        for t in threads:
            t.join()
        for index in range(len(second_gopts)):
            usage[1].append(second_gopts[index] + [ir_variant_arg[ir_variant_index]])

        ir_variant_index += 1
        second_output_files = tmp_output_files
    optimistic_final_len = min(min(len(op) for op in first_output_files), len(data) + 24)
    pessimistic_final_len = min(min(len(op) for op in second_output_files), len(data) + 24)
    for index in range(len(first_output_files)):
        if len(first_output_files[index]) == optimistic_final_len:
            break
    for index in range(len(second_output_files)):
        if len(second_output_files[index]) == pessimistic_final_len:
            break
    with lock:
        optimistic_divans_total += int(optimistic_final_len * weight)
        pessimistic_divans_total += int(pessimistic_final_len * weight)
        brotli_total += int(len(compressed) * weight)
        brotli_divans_hybrid += int(min(len(compressed), pessimistic_final_len) * weight)
        opt_brotli_divans_hybrid += int(min(len(compressed), optimistic_final_len) * weight)
        baseline_total += baseline_compression
        print 'stats:', pessimistic_final_len, 'vs', optimistic_final_len, 'vs', len(compressed), 'vs baseline:',baseline_compression, (usage[1][index] if index < len(usage[1]) else 'uncompressed')
        print 'sum:', pessimistic_divans_total, 'vs', optimistic_divans_total, 'vs', brotli_total, 'vs baseline:',baseline_total
        print 'opts:', [len(i) for i in first_output_files], path
        print 'args:', [len(i) for i in second_output_files], path
        print pessimistic_divans_total * 100 /float(brotli_total), '% opt: ', optimistic_divans_total*100/float(brotli_total),'% hybrid:', brotli_divans_hybrid *100/float(brotli_total),'% opt hybrid', opt_brotli_divans_hybrid *100/float(brotli_total),'% vs baseline ', pessimistic_divans_total*100/float(baseline_total), '%'
        sys.stdout.flush()

if __name__ == "__main__":
    main()

