import sys
import json
import traceback
total = {}
num_rows = 0

def summarize():
    print "Summary for",num_rows
    for key in sorted(total.keys()):
        temp = [x/num_rows for x in total[key]]
        temp[0] = total[key][0] * 100. /total['zlib'][0]
        print str(key) + ':' + str(temp)

for line in sys.stdin:
    try:
        row = json.loads(line)
    except Exception:
        traceback.print_exc()
        continue
    if row['zlib'][0] / float(row['~raw']) > .995:
        continue
    num_rows += 1
    for (key, value) in row.iteritems():
        if key not in total:
            total[key] = [0,0, 0]
        if key == '~path' or key=='~':
            continue
        if key == '~raw':
            total[key][0] += value
            continue
        total[key][0] += value[0]
        total[key][1] += value[1]
        total[key][2] += value[2]
    if num_rows % 100000 == 0:
        summarize()
summarize()
