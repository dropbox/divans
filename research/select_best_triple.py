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
        scores = [int(item.strip()) for item in line[6:].replace('baseline: ','').replace('vsIX','vs').replace('vsXI','vs').replace("vsX", "vs").replace('vsZstd','vs').replace('vsZ','vs').replace('vsU','vs').replace('vs:','vs').split('[')[0].split(' vs ')]
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
bytescore_required = ""
if len(sys.argv) > 1:
    bytescore_required = "-bytescore=" + sys.argv[1]
def not_ok(index):
    if index == 50:
        return True
    found_ok_byte_score = False
    for item in hdrs[index]:
        if bytescore_required in item or '-bytescore=340' in item:
            found_ok_byte_score = True
    return not found_ok_byte_score
    #if index >= 10:
    #    return True # force us to use brotli-9
    for item in hdrs[index]:
        if 'speedlow' in item:
            return True
    return False

#best_price = 0
def is_blacklisted(baseline, uncompressed):
    if baseline / float(uncompressed) > .995:
        return True
    if uncompressed - baseline < 128:
        return True
    return False
for include_ignored in [False, True]:
    perfect_prediction = 0
    num_options = len(samples[0])
    total_count = [0] * num_options
    brotli_total = 0
    brotli9_total = 0
    brotli10_total = 0
    brotli11_total = 0
    zstd_total = 0
    baseline_total = 0
    cost = 0
    favored = [0, 0, 0, 0, 0, 0]
    display = {}
    ignored = 0
    ignored_and_viewed = 0
    for favored_index in range(0,6):
        total_count = [0] * num_options
        for xindex in range(len(samples)):
            sample = [n for n in samples[xindex]]
            divans,brotli,brotli9, brotli10,brotli11,zstd,baseline ,uncompressed= othstats[xindex]
            blacklist = is_blacklisted(baseline, uncompressed)
            ignored_and_viewed+= baseline
            if blacklist:
                ignored += baseline
                if not include_ignored:
                    continue # ignore these samples--assume they are vids
                divans,brotli,brotli9, brotli10,brotli11,zstd = baseline, baseline, baseline, baseline, baseline, baseline
                for index in range(len(sample)):
                    sample[index] = baseline
            if favored_index == 0:
                target = min([sample[index] for index in range(len(sample)) if not not_ok(index)]+ [baseline])
                perfect_prediction += target
                baseline_total += baseline
                brotli_total += brotli
                brotli9_total += brotli9
                brotli10_total += brotli10
                brotli11_total += brotli11
                zstd_total += zstd
                cost += max(sample)
            else:
                target = min(sample)
            for index in range(num_options):
                cur = min([baseline] + [sample[index]] + [sample[fav] for fav in favored[:favored_index]])
                if not_ok(index):
                    total_count[index] += cur * 1000
                else:
                    total_count[index] += cur
    
        for index in range(num_options):
            if total_count[index] < cost:
                cost = total_count[index]
                favored[favored_index] = index
        print cost / 1000.
    if include_ignored:
        print 'using zlib for % of bytes ',ignored * 100./ignored_and_viewed
    else:
        print 'ignored % bytes ',ignored * 100./ignored_and_viewed
    print 'perfect', perfect_prediction / 1000., 'brotli',brotli_total/1000.,'brotli9',brotli9_total/1000.,'brotli10',brotli10_total/1000.,'brotli11',brotli11_total/1000.,'zstd',zstd_total/1000.,'baseline',baseline_total/1000.
    print 'pct vs brotli', cost * 100. / brotli_total
    print 'pct vs brotli9', cost * 100. / brotli9_total
    print 'pct vs brotli10', cost * 100. / brotli10_total
    print 'pct vs brotli11', cost * 100. / brotli11_total
    print 'pct vs zstd', cost * 100. / zstd_total
    print 'pct vs zlib', cost * 100. / baseline_total
    #print json.dumps(display,indent=2)
    print favored
    
    print [hdrs[favor] for favor in favored]

