import sys
from collections import defaultdict

features = defaultdict(lambda:0)
remap = {
    'CopyDistance':'CopyDistance',
    'DistanceHuffmanTable':'CopyDistance',
    'ComplexLiterals':'ComplexLiterals',
    'CopyLength':'CopyLength',
    'LiteralHuffmanTable':'ComplexLiterals',
    'InsertCopyHuffmanTable':'CopyLength',
    'LiteralContextMode':'LiteralContextMode',
    'MetablockHeader':'Misc',
    'BlockTypeMetadata':'BlockTypeMetadata',
    'DistancContextMode':'DistanceContextMode',
    'Misc':'Misc',
    }

for line in open(sys.argv[1]):
    for key,val in remap.iteritems():
        if key != val:
            line = line.replace(key,val)
    vals = line.split()
    bytes = float(vals[1])
    features[vals[2]] += bytes
maxb = max(len(str(item)) for item in features.values())
maxa = max(len(str(int(item*8 + .5))) for item in features.values())

for item in sorted(features.keys()):
    bitval = str(int(features[item] * 8 + .5))
    byteval = str(features[item])
    print bitval + ' '*(maxa + 2 - len(bitval)) + byteval + ' '*(maxb + 2 - len(byteval)) + item
