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
    print "Summary for",num_rows,'Processed ',(uncut * 100.)/(cut + uncut),'%', raw_size / 1000.**4
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
            print str(key) + ': ' + str(total[key][0]) + '/' + str(total['zlib'][0]) + ' vs raw ' + str(total[key][0]) + '/' + str(total['~raw'][0])
            decode_pct[key] = [1000 * val[p9999], 1000 * val[p99], 1000 * val[p75], 1000 * val[p50]]
    if show_results:
        try:
            import divansplot
            
        except Exception:
            traceback.print_exc()
            show_results = False
    if show_results:
        divansplot.draw(ratio_vs_raw, ratio_vs_zlib, encode_avg, decode_avg, decode_pct)
gopts_map = {
    'd1':[['-O2', '-q11', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
          ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
           '-sign', '-speed=32,4096'],
          ['-O2', '-q10', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-sign', '-speed=16,8192'],
          ['-O2', '-q11', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
          ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
           '-lsb', '-speed=2,1024']],
    'd12':[['-O2', '-q9.5', '-w22', '-defaultprior', '-lgwin22', '-mixing=2', '-bytescore=340']],
    'd13':[ ['-O2', '-q9.5', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-speed=2,2048', '-bytescore=540'],
            ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-bytescore=140', '-speed=32,4096']],
    'd15':[['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-speed=2,2048', '-bytescore=840'],
           ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-bytescore=340'],
           ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-bytescore=140', '-speed=32,4096']],
    'd20':[['-O2', '-q10', '-w22', '-lsb', '-lgwin22', '-mixing=1', '-findprior', '-speed=2,2048'],
           ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=140',
            '-sign', '-speed=32,4096'],
           ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=40',
            '-sign', '-speed=16,8192'],
           ['-O2', '-q10', '-w22', '-lgwin18', '-mixing=1', '-findprior', '-speed=16,8192'],
           ['-O2', '-q9.5', '-w22', '-lgwin22', '-mixing=1', '-findprior', '-bytescore=340',
            '-lsb', '-speed=2,1024']],
    'd21':[['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=140'],
           ['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
           ['-q9', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=840', "-speed=16,8192"]],
    'd29':[['-q9', '-defaultprior', '-nocm', '-w22', '-lgwin22', '-mixing=0', '-bytescore=340']],
    'd35':[ ['-q7', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=40'],
            ['-q7', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340'],
            ['-q7', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=540'],
            ['-q7', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=840', '-speed=1,16384']],
    'd38':[['-q5', '-defaultprior', '-w22', '-lgwin22', '-mixing=2', '-bytescore=340']],
    
    }
for line in sys.stdin:
    if sys.argv[1] == '--cut':
        line = line[line.find(':') + 1:]
    try:
        row = json.loads(line)
    except Exception:
        traceback.print_exc()
        continue
    zlib_ratio = row['zlib'][0] / float(row['~raw'])
    if sys.argv[2] == "image":
        if row['zlib'][0] == row['~raw']:
            cut += row['zlib'][0]
            continue
    elif zlib_ratio > float(sys.argv[2])/100.:
        cut += row['zlib'][0]
        continue
    uncut += row['zlib'][0]
    raw_size += row['~raw']
    mb_size = row['~raw']/1024./1024.
    if mb_size < .01:
        cut += row['zlib'][0]
        continue
    num_rows += 1
    candidate = [0,0,0]
    #rule = ['d12',
    #        'd13',
    #        'd20',
    #        'd21',
    #        'd1',
    #        'd1']
    #if zlib_ratio > .99:
    #    candidate = row[rule[5]]
    #elif zlib_ratio > .96:
    #    candidate = row[rule[4]]
    #elif zlib_ratio > .92: # .85
    #    candidate = row[rule[3]]
    #elif zlib_ratio > .89: # .25
    #    candidate = row[rule[2]]
    #elif zlib_ratio > .85: # .22
    #    candidate = row[rule[1]]
    #else:
    #    candidate = row[rule[0]] #1
    #row['dY'] = candidate
    #if zlib_ratio > .97:
    #    candidate = row['d29'] # fast to encode fast to decode
    #elif zlib_ratio > .9:
    #    candidate = row['d38'] # fastest to encode slow to decode
    #elif zlib_ratio > .5:
    #    candidate = row['d35'] # fast to encode and slow to decode
    #else:
    #    candidate = row['d15'] # slow to encode fast to decode
    #    candidate = row['d1'] # slowest to encode fat to decoed
    #row['dX'] = candidate
    
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
        if mb_size >  .01:
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
