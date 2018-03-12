import os
import sys
import subprocess
import threading
import random

walk_dir = sys.argv[1]
divans = sys.argv[2]
other = sys.argv[3]
speeds = ["0,32", "1,32", "1,128", "1,16384", "2,1024", "4,1024", "8,8192", "16,48", "16,8192", "32,4096", "64,16384", "128,256", "128,16384", "512,16384", "1664,16384"]
gopts = [['-cm', '-findspeed'], #0
         ['-s', '-cm','-mixing=2','-stride=1', '-findspeed'], #1
         ['-s','-stride=1', '-speed=MUD'],
         ['-s','-brotlistride', '-speed=MUD'],
         ['-s', '-cm','-mixing=2', '-brotlistride', '-findspeed'],
         ['-cm', '-speed=' + speeds[0]],#5
         ['-cm', '-speed=' + speeds[1]],
         ['-cm', '-speed=' + speeds[2]],
         ['-cm', '-speed=' + speeds[3]],
         ['-cm', '-speed=' + speeds[4]],
         ['-cm', '-speed=' + speeds[5]],
         ['-cm', '-speed=' + speeds[6]],
         ['-cm', '-speed=' + speeds[7]],
         ['-cm', '-speed=' + speeds[8]],#13 lazy
         ['-cm', '-speed=' + speeds[9]],
         ['-cm', '-speed=' + speeds[10]],
         ['-cm', '-speed=' + speeds[11]],
         ['-cm', '-speed=' + speeds[12]],
         ['-cm', '-speed=' + speeds[13]],
         ['-cm', '-speed=' + speeds[14]],#19
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
]
lock = threading.Lock()
brotli_divans_hybrid = 0
brotli_total = 0
divans_total = 0
best_cm_total=0
shortcut_cm_total = 0
lazy_cm_total = 0
best_mix_total=0
shortcut_mix_total = 0
lazy_mix_total = 0
def start_thread(path, exe, uncompressed, ir, out_array, gopts, index):
    def start_routine():
        compressor = subprocess.Popen([exe, '-i'] + gopts[index], stdout=subprocess.PIPE, stdin=subprocess.PIPE)
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
            process_file(path, data, metadata.st_size/float(len(data)))
def process_file(path, data, weight=1):
    global lock
    global brotli_total
    global brotli_divans_hybrid
    global divans_total
    global best_cm_total
    global shortcut_cm_total
    global lazy_cm_total
    global best_mix_total
    global shortcut_mix_total
    global lazy_mix_total

    otherp = subprocess.Popen([other, '-c', '-i', '-basicstride', '-findspeed'], stdout=subprocess.PIPE, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
    compressed, ir = otherp.communicate(data)
    exit_code = otherp.wait()
    if exit_code != 0:
        print ir
    assert exit_code == 0
    output_files = [''] * len(gopts)
    threads = []
    for index in range(len(output_files)):
        threads.append(start_thread(path, divans, data, ir, output_files, gopts, index))
    for t in threads:
        t.join()
    final_len = min(min(len(op) for op in output_files), len(data) + 24)
    best_speed_cm = min(len(op) for op in output_files[5:20])
    best_shortcut_cm = len(output_files[0])
    best_speed_mix = min(len(op) for op in output_files[20:])
    best_shortcut_mix = len(output_files[1])
    for index in range(len(output_files)):
        if len(output_files[index]) == final_len:
            break
    with lock:
        best_cm_total += int(best_speed_cm*weight)
        shortcut_cm_total += int(best_shortcut_cm*weight)
        lazy_cm_total += int(len(output_files[13])* weight)
        best_mix_total += int(best_speed_mix*weight)
        shortcut_mix_total += int(best_shortcut_mix*weight)
        lazy_mix_total += int(len(output_files[28])*weight)
        divans_total += int(final_len * weight)
        brotli_total += int(len(compressed) * weight)
        brotli_divans_hybrid += int(min(len(compressed), final_len) * weight)
        print final_len, 'vs', len(compressed), (gopts[index] if index < len(gopts) else 'uncompressed'),[len(i) for i in output_files], path
        print divans_total * 100 /float(brotli_total), '% hybrid: ', brotli_divans_hybrid*100/float(brotli_total),'%  cm:', float(shortcut_cm_total)*100/float(best_cm_total), '% mix: ', shortcut_mix_total*100/float(best_mix_total),'% lazy:',lazy_mix_total*100/float(best_mix_total),'%'
        sys.stdout.flush()

if __name__ == "__main__":
    main()

