import sys
import json
import traceback
from collections import defaultdict
total = {}
num_rows = 0
raw_size =0 

cut = 0
uncut = 0
decode_hist = defaultdict(list)
def summarize():
    print "Summary for",num_rows,'Processed ',(uncut * 100.)/(cut + uncut),'%'
    for key in sorted(total.keys()):
        temp = [total[key][0] * 100. /total['zlib'][0],
                total[key][3]/max(total[key][1], 1),
                total[key][3]/max(total[key][2], 1)]
        print str(key) + ':' + str(temp), 'sav', str((total[key][0] + cut) * 100./ (cut + uncut)) + '%'
        if key in decode_hist:
            val = decode_hist[key]
            val.sort()
            vlen = len(val)
            p9999 = vlen * 9999 // 10000
            p99 = vlen * 99 // 100
            p95 = vlen * 95 // 100
            p75 = vlen * 75//100
            p50 = vlen // 2
            print str(key) + ': P50:' + str(val[p50]) + ' P75:' + str(val[p75]) +' P99:' + str(val[p99]) + ' P9999:' + str(val[p9999])

for line in sys.stdin:
    if sys.argv[1] == '--cut':
        line = line[line.find(':') + 1:]
    try:
        row = json.loads(line)
    except Exception:
        traceback.print_exc()
        continue
    if row['zlib'][0] / float(row['~raw']) > float(sys.argv[2])/100.:
        cut += row['zlib'][0]
        continue
    uncut += row['zlib'][0]
    raw_size += row['~raw']
    mb_size = row['~raw']/1024./1024.
    num_rows += 1

    for (key, value) in row.iteritems():
        if key not in total:
            total[key] = [0,0,0,0]
        if key == '~path' or key=='~':
            continue
        if key == '~raw':
            total[key][0] += value
            continue
        total[key][0] += value[0]
        decode_hist[key].append(value[2])
        if mb_size >= 1:
            total[key][1] += value[1]
            total[key][2] += value[2]
            total[key][3] += mb_size
    if num_rows % 100000 == 0:
        summarize()
summarize()
