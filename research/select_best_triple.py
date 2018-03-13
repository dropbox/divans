import json
import sys
samples = []
command_map = {}
for line in sys.stdin:
    where = line.find('[')
    if where == -1:
       continue
    where2 = line.find(']')
    command_line = line[where:where2]
    where = line.find('[', where + 1)
    if where == -1:
       continue
    where2 = line.find(']', where + 1)
    if where2 == -1:
       continue
    json_src = json.loads(line[where:where2 + 1])
    best_item = min(json_src)
    for index in range(len(json_src)):
        if json_src[index] == best_item:
            command_map[index] = command_line
            break
    samples.append(json_src)

perfect_prediction = 0
num_options = len(samples[0])
best_count = [0] * num_options
total_count = [0] * num_options
cost = 0
#best_price = 0
for sample in samples:
    target = min(sample)
    perfect_prediction += target
    cost += max(sample)
    for index in range(num_options):
        total_count[index] += sample[index]
        if sample[index] <= target * 1001 / 1000:
            best_count[index] += sample[index]
favored = [0, 0, 0, 0, 0]
display = {}
print cost / 1000.
for index in range(num_options):
    if total_count[index] < cost:
        cost = total_count[index]
        favored[0] = index
for favored_index in range(1,5):
    best_count = [0] * num_options
    total_count = [0] * num_options
    for sample in samples:
        target = min(sample)
        for index in range(num_options):
            cur = min([sample[index]] + [sample[fav] for fav in favored[:favored_index]])
            total_count[index] += cur
            if cur <= target * 1001 / 1000:
                best_count[index] += cur

    for index in range(num_options):
        if total_count[index] < cost:
            cost = total_count[index]
            favored[favored_index] = index
    print cost / 1000.
print 'perfect', perfect_prediction / 1000.
#print json.dumps(display,indent=2)
print favored

#print [command_map.get(favor) for favor in favored]
