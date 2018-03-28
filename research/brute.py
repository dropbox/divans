import sys
import threading
import subprocess
import Queue
ir = sys.stdin.read()
found_mixing_offsets = []
start = 0
def run(output_q, procedure, input):
    so, se = procedure.communicate(input)
    output_q.put(so)

while True:
    key = "mixingvalues "
    where = ir.find(key, start)
    if where == -1:
        if start == 0:
            assert where != -1, "Must have at least one mixingvalues"
        break
    for end_index in range(where + len(key), len(ir)):
        if ir[end_index] not in ('0', '1', ' '):
            break
    found_mixing_offsets.append((where + len(key), end_index))
    start = where + 1

q = Queue.Queue()
for item in found_mixing_offsets:
    array = ['0 '] * 256;
    for sub_offset in range(256):
        array[sub_offset] = '0 '
        option_a = ''.join(array)
        array[sub_offset] = '1 '
        option_b = ''.join(array)
        ir_a = ir[:item[0]] + option_a + ir[item[1]:]
        ir_b = ir[:item[0]] + option_b + ir[item[1]:]
        proc_a = subprocess.Popen([sys.argv[1],
                                   '-i', '-cm', '-s', '-mixing=1'] + sys.argv[2:],
                                  stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE)
        proc_b = subprocess.Popen([sys.argv[1],
                                   '-i', '-cm', '-s', '-mixing=1'] + sys.argv[2:],
                                  stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE)
        threading.Thread(target=lambda: run(q, proc_a, ir_a)).start()
        b_stdout, _stderr = proc_b.communicate(ir_b)
        a_ec = proc_a.wait()
        b_ec = proc_b.wait()
        if a_ec != 0 or b_ec != 0:
            with open('/tmp/ira','w') as f:
                f.write(ir_a)
            with open('/tmp/irb','w') as f:
                f.write(ir_b)
        assert a_ec == 0
        assert b_ec == 0
        a_stdout = q.get()
        if len(a_stdout) < len(b_stdout):
            array[sub_offset] = '0 '
            sys.stderr.write("index " + str(sub_offset) + "Prefer 0 for " + str(len(a_stdout)) + "\n")
        else:
            sys.stderr.write("index " + str(sub_offset) + "Prefer 1 for "+ str(len(b_stdout)) + "\n")
            array[sub_offset] = '1 '
    ir = ir[:item[0]] + ''.join(array) + ir[item[1]:]

sys.stdout.write(ir)
