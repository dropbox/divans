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
dig5 = 10000000.0
def prec(x, scale=100000.0):
    return int(x * scale +.5)/scale

def summarize(show_results=True):
    print "Summary for",num_rows,'Processed ',(uncut * 100.)/(cut + uncut),'%'
    ratio_vs_zlib = {}
    ratio_vs_raw = {}
    encode_avg = {}
    decode_avg = {}
    decode_st_avg = {}
    decode_pct = {}
    for key in sorted(total.keys()):
        temp = [total[key][0] * 100. /total['zlib'][0],
                total[key][3]/max(total[key][1], 1),
                total[key][3]/max(total[key][2], 1),
                total[key][3]/max(total[key][4], 1)]
        print str(key) + ':' + str([prec(t) for t in temp]), 'sav', str(prec((total[key][0] + cut) * 100./ (cut + uncut))) + '%'
        ratio_vs_zlib[key] = [100 - 100. * float(total[key][0])/total['zlib'][0]]
        ratio_vs_raw[key] =  [100 - 100. * float(total[key][0])/total['~raw'][0]]
        encode_avg[key] = [8 * total[key][3]/max(total[key][1], .00001)]
        decode_avg[key] = [8 * total[key][3]/max(total[key][2], .00001)]
        decode_st_avg[key] = [8 * total[key][3]/max(total[key][4], .00001)]
        if key in decode_hist:
            val = decode_hist[key]
            val.sort()
            vlen = len(val)
            p9999 = vlen * 9999 // 10000
            p99 = vlen * 99 // 100
            p95 = vlen * 95 // 100
            p75 = vlen * 75//100
            p50 = vlen // 2
            print str(key) + ': P50:' + str(prec(val[p50],dig5)) + ' P75:' + str(prec(val[p75], dig5)) +' P99:' + str(prec(val[p99], dig5)) + ' P9999:' + str(prec(val[p9999], dig5))
            decode_pct[key] = [1000 * val[p9999], 1000 * val[p99], 1000 * val[p75], 1000 * val[p50]]
    if show_results:
        try:
            import divansplot
            
        except Exception:
            traceback.print_exc()
            show_results = False
    if show_results:
        divansplot.draw(ratio_vs_raw, ratio_vs_zlib, encode_avg, decode_avg, decode_pct)

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
            total[key] = [0,0,0,0,0]
        if key == '~path' or key=='~':
            continue
        if key == '~raw':
            total[key][0] += value
            continue
        total[key][0] += value[0]
        decode_hist[key].append(value[2])
        if mb_size >= 1 or True:
            total[key][1] += value[1]
            total[key][2] += value[2]
            total[key][3] += mb_size
            if len(value) > 3:
                total[key][4] += value[3]
            else:
                total[key][4] += value[2]
    if num_rows % 100000 == 0:
        summarize(False)
summarize()
