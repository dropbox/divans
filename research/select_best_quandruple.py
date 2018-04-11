import json
import sys
from collections import defaultdict
cut = True
sub_item = 5
combo_scores = defaultdict(lambda:0)
data_list = []
score_record = []
for line in sys.stdin:
    try:
        if cut:
            line = line[line.find(':')+1:]
        raw = json.loads(line)
        if raw['~raw']*.995 < raw['zlib'][0]:
            continue
        clist = raw['~'][sub_item]
        data_list.append(clist)
        for k0 in range(len(clist) - 1):
            for k1 in range(k0 + 1, len(clist)):
                key = (k0, k1)
                score = min(clist[k0][0], clist[k1][0])
                combo_scores[key] += score
    except Exception:
        continue
best_combo = min([(v, k[0], k[1]) for k, v in combo_scores.iteritems()])
score_record.append(best_combo[0])
best_elements = [best_combo[1], best_combo[2]]
combo_scores = defaultdict(lambda:0)
for sample in data_list:
    for k in range(len(sample)):
        combo_scores[k] += min(sample[best_elements[0]][0],
                               sample[best_elements[1]][0],
                               sample[k][0])
best_val = min([(v,k) for k, v in combo_scores.iteritems()])
score_record.append(best_val[0])
best_elements.append(best_val[1])
combo_scores = defaultdict(lambda:0)
for sample in data_list:
    for k in range(len(sample)):
        combo_scores[k] += min(sample[best_elements[0]][0],
                               sample[best_elements[1]][0],
                               sample[best_elements[2]][0],
                               sample[k][0])
best_val = min([(v,k) for k, v in combo_scores.iteritems()])
score_record.append(best_val[0])
best_elements.append(best_val[1])
combo_scores = defaultdict(lambda:0)
for sample in data_list:
    for k in range(len(sample)):
        combo_scores[k] += min(sample[best_elements[0]][0],
                               sample[best_elements[1]][0],
                               sample[best_elements[2]][0],
                               sample[best_elements[3]][0],
                               sample[k][0])
best_val = min([(v,k) for k, v in combo_scores.iteritems()])
score_record.append(best_val[0])
best_elements.append(best_val[1])
combo_scores = defaultdict(lambda:0)
for sample in data_list:
    for k in range(len(sample)):
        combo_scores[k] += min(sample[best_elements[0]][0],
                               sample[best_elements[1]][0],
                               sample[best_elements[2]][0],
                               sample[best_elements[3]][0],
                               sample[best_elements[4]][0],
                               sample[k][0])
best_val = min([(v,k) for k, v in combo_scores.iteritems()])
score_record.append(best_val[0])
best_elements.append(best_val[1])
combo_scores = defaultdict(lambda:0)
prescient_score = 0
for sample in data_list:
    prescient_score += min(x[0] for x in sample)
    for k in range(len(sample)):
        combo_scores[k] += min(sample[best_elements[0]][0],
                               sample[best_elements[1]][0],
                               sample[best_elements[2]][0],
                               sample[best_elements[3]][0],
                               sample[best_elements[4]][0],
                               sample[best_elements[5]][0],
                               sample[k][0])
best_val = min([(v,k) for k, v in combo_scores.iteritems()])
score_record.append(best_val[0])
best_elements.append(best_val[1])
print best_elements,'score',score_record,'best',prescient_score
