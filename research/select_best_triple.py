import json
import sys
samples = []
othstats = []
hdrs = []
for line in sys.stdin:
    if line.startswith('hdr:'):
        hdrs = json.loads(line[line.find(':')+1:].replace("'",'"'))
        continue
    if line.startswith('stats:'):
        scores = [int(item.strip()) for item in line[6:].replace('baseline: ','').replace('vsIX','vs').replace('vsXI','vs').replace("vsX", "vs").split('[')[0].split(' vs ')]
        othstats.append(scores)
    if not line.startswith('args:'):
        continue # ignore anything but the nonopt items
    where = line.find('[')
    if where == -1:
       continue
    where2 = line.find(']')
    json_src = json.loads(line[where:where2 + 1])
    best_item = min(json_src)
    for index in range(len(json_src)):
        if json_src[index] == best_item:
            break
    samples.append(json_src)
def not_ok(index):
    for item in hdrs[index]:
        if 'speedlow' in item:
            return True
    return False

perfect_prediction = 0
num_options = len(samples[0])
best_count = [0] * num_options
total_count = [0] * num_options
brotli_total = 0
brotli9_total = 0
brotli10_total = 0
brotli11_total = 0
baseline_total = 0
cost = 0
#best_price = 0
for xindex in range(len(samples)):
    sample = samples[xindex]
    divans,brotli,brotli9,brotli10,brotli11,baseline = othstats[xindex]
    target = min(sample + [baseline])
    perfect_prediction += target
    baseline_total += baseline
    brotli_total += brotli
    brotli9_total += brotli9
    brotli10_total += brotli10
    brotli11_total += brotli11
    cost += max(sample)
    best_index = -1
    for index in range(num_options):
        if not_ok(index):
            total_count[index] += min(sample[index], baseline) * 10
        else:
            total_count[index] += min(sample[index], baseline)
        if sample[index] <= target * 1001 / 1000:
            best_count[index] += sample[index]
            best_index = index
favored = [0, 0, 0, 0, 0, 0]
display = {}
print cost / 1000.
for index in range(num_options):
    if total_count[index] < cost:
        cost = total_count[index]
        favored[0] = index
for favored_index in range(1,6):
    best_count = [0] * num_options
    total_count = [0] * num_options
    for xindex in range(len(samples)):
        sample = samples[xindex]
        divans,brotli,brotli9, brotli10,brotli11,baseline = othstats[xindex]
        target = min(sample)
        for index in range(num_options):
            cur = min([baseline] + [sample[index]] + [sample[fav] for fav in favored[:favored_index]])
            if not_ok(index):
                total_count[index] += cur * 1000
            else:
                total_count[index] += cur
            if cur <= target * 1001 / 1000:
                best_count[index] += cur

    for index in range(num_options):
        if total_count[index] < cost:
            cost = total_count[index]
            favored[favored_index] = index
    print cost / 1000.
print 'perfect', perfect_prediction / 1000., 'brotli',brotli_total/1000.,'brotli9',brotli9_total/1000.,'brotli10',brotli10_total/1000.,'brotli11',brotli11_total/1000.,'baseline',baseline_total/1000.
print 'pct vs brotli', cost * 100. / brotli_total
print 'pct vs brotli9', cost * 100. / brotli9_total
print 'pct vs brotli10', cost * 100. / brotli10_total
print 'pct vs brotli11', cost * 100. / brotli11_total
print 'pct vs zlib', cost * 100. / baseline_total
#print json.dumps(display,indent=2)
print favored

print [hdrs[favor] for favor in favored]
