import json
import sys
from collections import defaultdict
cut = True
best_other_alg = 'zlib'
if len(sys.argv) > 1 and 'b95' in sys.argv[1]:
    best_other_alg = 'b95'
elif len(sys.argv) > 1 and 'b11' in sys.argv[1]:
    best_other_alg = 'b11'
elif len(sys.argv) > 1:
    assert 'zlib' in sys.argv[1]
sub_item = 6
combo_scores = defaultdict(lambda:0)
data_list = []
zlib_other_list = []
score_record = []

for line in sys.stdin:
    try:
        if cut:
            line = line[line.find(':')+1:]
        raw = json.loads(line)
        b11_cost = raw['b11'][0]
        b95_cost = raw['b95'][0]
        zlib_cost = raw['zlib'][0]
        other_cost = raw[best_other_alg][0]
        if raw['~raw']*.995 < zlib_cost:
            continue
        clist = raw['~'][sub_item]
        data_list.append(clist)
        zlib_other_list.append((zlib_cost, other_cost))
        for k0 in range(len(clist) - 1):
            for k1 in range(k0 + 1, len(clist)):
                key = (k0, k1)
                score = min(clist[k0][0], clist[k1][0], other_cost, zlib_cost)
                combo_scores[key] += score
    except Exception:
        continue
best_combo = min([(v, k[0], k[1]) for k, v in combo_scores.iteritems()])
score_record.append(best_combo[0])
best_elements = [best_combo[1], best_combo[2]]
print 'partial', best_elements,'score',score_record
sys.stdout.flush()
combo_scores = defaultdict(lambda:0)
for (sample, other) in zip(data_list, zlib_other_list):
    for k in range(len(sample)):
        combo_scores[k] += min(sample[best_elements[0]][0],
                               sample[best_elements[1]][0],
                               sample[k][0], other[0], other[1])
best_val = min([(v,k) for k, v in combo_scores.iteritems()])
score_record.append(best_val[0])
best_elements.append(best_val[1])
print 'partial', best_elements,'score',score_record
sys.stdout.flush()
combo_scores = defaultdict(lambda:0)
for (sample, other) in zip(data_list, zlib_other_list):
    for k in range(len(sample)):
        combo_scores[k] += min(sample[best_elements[0]][0],
                               sample[best_elements[1]][0],
                               sample[best_elements[2]][0],
                               sample[k][0], other[0], other[1])
best_val = min([(v,k) for k, v in combo_scores.iteritems()])
score_record.append(best_val[0])
best_elements.append(best_val[1])
print 'partial', best_elements,'score',score_record
sys.stdout.flush()
combo_scores = defaultdict(lambda:0)

for (sample, other) in zip(data_list, zlib_other_list):
    for k in range(len(sample)):
        combo_scores[k] += min(sample[best_elements[0]][0],
                               sample[best_elements[1]][0],
                               sample[best_elements[2]][0],
                               sample[best_elements[3]][0],
                               sample[k][0], other[0], other[1])
best_val = min([(v,k) for k, v in combo_scores.iteritems()])
score_record.append(best_val[0])
best_elements.append(best_val[1])
print 'partial', best_elements,'score',score_record
sys.stdout.flush()
combo_scores = defaultdict(lambda:0)
for (sample, other) in zip(data_list, zlib_other_list):
    for k in range(len(sample)):
        combo_scores[k] += min(sample[best_elements[0]][0],
                               sample[best_elements[1]][0],
                               sample[best_elements[2]][0],
                               sample[best_elements[3]][0],
                               sample[best_elements[4]][0],
                               sample[k][0], other[0], other[1])
best_val = min([(v,k) for k, v in combo_scores.iteritems()])
score_record.append(best_val[0])
best_elements.append(best_val[1])
print 'partial', best_elements,'score',score_record
sys.stdout.flush()
combo_scores = defaultdict(lambda:0)
prescient_score = 0
for (sample, other) in zip(data_list, zlib_other_list):
    prescient_score += min(min(x[0] for x in sample), min(other))
    for k in range(len(sample)):
        combo_scores[k] += min(sample[best_elements[0]][0],
                               sample[best_elements[1]][0],
                               sample[best_elements[2]][0],
                               sample[best_elements[3]][0],
                               sample[best_elements[4]][0],
                               sample[best_elements[5]][0],
                               sample[k][0], other[0], other[1])
best_val = min([(v,k) for k, v in combo_scores.iteritems()])
score_record.append(best_val[0])
best_elements.append(best_val[1])
print best_elements,'score',score_record,'best',prescient_score
