import sys
import json
import traceback
total = {}
num_rows = 0
raw_size =0 

def summarize():
    print "Summary for",num_rows
    for key in sorted(total.keys()):
        temp = [total[key][0] * 100. /total['zlib'][0],
                total[key][3]/max(total[key][1], 1),
                total[key][3]/max(total[key][2], 1)]
        print str(key) + ':' + str(temp)

for line in sys.stdin:
    if sys.argv[1] == '--cut':
        line = line[line.find(':') + 1:]
    try:
        row = json.loads(line)
    except Exception:
        traceback.print_exc()
        continue
    if row['zlib'][0] / float(row['~raw']) > .995:
        continue
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
        if mb_size >= 1:
            total[key][1] += value[1]
            total[key][2] += value[2]
            total[key][3] += mb_size
    if num_rows % 100000 == 0:
        summarize()
summarize()
