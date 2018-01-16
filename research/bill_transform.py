import sys
from collections import defaultdict

features = defaultdict(lambda:0)
remap = {
    'CountLengthFirst': 'CopyLength',
    'CountMantissaNibbles': 'CopyLength',
    'CountSmall': 'CopyLength',
    'DistanceLengthFirst': 'CopyDistance',
'DistanceLengthGreater15Less25': 'CopyDistance',
'DistanceLengthMnemonic': 'CopyDistance',
'DistanceMantissaNibbles': 'CopyDistance',
    'BlockSwitchType': 'BlockTypeMetadata',
    'FullSelection': 'CopyLength',  # not quite truthful
    'LiteralCountFirst': 'CopyLength', # not quite truthful
    'LiteralCountLengthGreater14Less25': 'CopyLength', # not quite truthful
    'LiteralCountMantissaNibbles': 'CopyLength', # not quite truthful
    'LiteralCountSmall': 'CopyLength', # not quite truthful
    'LiteralNibbleIndex': 'ComplexLiterals',
    'LiteralNibbleIndex': 'ComplexLiterals',
    'Begin': 'Misc',
    'ContextMapFirstNibble(0, Literal)': 'LiteralContextMode',
    'ContextMapFirstNibble(0, Distance)': 'DistanceContextMode',
    'ContextMapMnemonic(0, Literal)': 'LiteralContextMode',
    'ContextMapMnemonic(0, Distance)': 'DistanceContextMode',
    'ContextMapSecondNibble(0, Literal, 0)': 'LiteralContextMode',
    'ContextMapSecondNibble(0, Distance, 0)': 'DistanceContextMode',
    'DynamicContextMixing': 'Misc',
    'LiteralAdaptationRate': 'Misc',
    'PriorDepth': 'Misc',
    }
for line in open(sys.argv[1]):
    if 'Total' in line:
        break
    pairmatch = '('.join(line.split('(')[1:]).split(')')[:-1]
    typ = ')'.join(pairmatch)
    counts = line.split('count:')[1:]
    byte_count_str = counts[1].strip().split(' ')[0].strip()
    byte_count = float(byte_count_str)
    if typ not in remap:
        typ = typ.split('(')[0]
    if typ not in remap:
        print typ, 'not found'
        continue
    features[remap[typ]] += byte_count
maxb = max(len(str(item)) for item in features.values())
maxa = max(len(str(int(item*8 + .5))) for item in features.values())
for item in sorted(features.keys()):
    bitval = str(int(features[item] * 8 + .5))
    byteval = str(features[item])
    print bitval + ' '*(maxa + 2 - len(bitval)) + byteval + ' '*(maxb + 2 - len(byteval)) + item
