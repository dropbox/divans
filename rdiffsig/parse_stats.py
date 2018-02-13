import sys
from collections import defaultdict

def main():
    result_dict = defaultdict(lambda:defaultdict(lambda:0))
    header = None
    for line in sys.stdin:
        csv = line.strip().split(',')
        if not header:
            header = csv
            continue
        key = csv[0] + csv[2]
        just_a_header = False
        for index in range(len(csv)):
            if header[index] not in ('addblocks', 'bits_sig', 'block_size', 'xid', 'xt'):
                try:
                    result_dict[key][header[index]] += int(csv[index])
                except ValueError:
                    just_a_header = True
                    break
        if just_a_header:
            continue
    for key in sorted([k.replace('1024', "ths").replace('2048','twt') for k in result_dict.keys()]):
        d = result_dict[key.replace('twt','2048').replace('ths','1024')]
        for skey in sorted(d.keys()):
            try:
                print key, skey, 100 * float(d[skey]) / float(d['raw_zlib'])
            except Exception:
                sys.stderr.write('bad' + str(d) + '\n')
                sys.stderr.flush()
                continue
            sys.stdout.flush()

if __name__ == '__main__':
    main()

