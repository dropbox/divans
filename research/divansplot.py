
import matplotlib.pyplot as plt 
from matplotlib import rcParams                                                                     
                          
import matplotlib.patches as patches
import matplotlib.transforms as transforms                                                                          
import numpy as np
from matplotlib.ticker import ScalarFormatter
def on_whitelist(key, label):
    #if 'key' == 'time_pct':
    #    return label in ('b11, d0')
    return label in ('b11', 'b9', 'd0', 'dX', 'zlib', 'z19', 'lzma', 'bz')
def label_reassign(key):
    keymap = {
        'b11': 'Brotli\nq11',
        'b9': 'Brotli\nq9',
        'd0': u'DivANS  .\nq11',
        'dX': u'DivANS\nq9',
        'd5': u'DivANS\nq9',
        'd35': u'DivANS\nq9',
        'z19': 'Zstd\nq19',
        'lzma': '7zip',
        'bz': 'bz2',
        }
    if key in keymap:
        return keymap[key]
    return key
colors = [[r for r in reversed(['#aaaaff','#9999dd','#4444aa','#000088',])],                        
           [r for r in reversed(['#ffffaa','#cccc88','#aaaa44','#999900',])],
           [r for r in reversed(['#ffaaaa','#cc8888','#aa4444','#880000',])],
           [r for r in reversed(['#aaffaa','#88cc88','#44aa44','#008800',])],
           [r for r in reversed(['#666666','#666666','#666666','#666666',])],
]
map_color = {
    'd0':colors[0][3],
    'd1':colors[0][3],
    'd2':colors[0][3],
    'd3':colors[0][2],
    'd4':colors[0][2],
    'dX':colors[0][2],
    'd5':colors[0][2],
    'd35':colors[0][2],
    'b9':colors[1][0],
    'b11':colors[1][1],
    'z19':colors[2][1],
    'zlib':colors[4][1],
    'bz':colors[3][1],
    'lzma':colors[3][1],
    }
ylabel = {
    'savings_vs_zlib':'% saving vs zlib\n',
    'encode_speed': 'Encode (Mbps)',
    'decode_speed': 'Decode (Mbps)',
    'time_pct':'Decode Time (ms)',
    }

y_limits= {
    'savings_vs_zlib':[0, 10],
    'encode_speed': [1,400],
    'decode_speed': [10,5000],
#    'time_pct':
    }
do_log = set(['decode_speed', 'encode_speed'])
def build_figure(key, ax, data, last=False):
    if key in do_log:
        ax.set_yscale('log')
    else:
        ax.set_yscale('linear')
    labels = []
    trans = transforms.blended_transform_factory(
        ax.transData, ax.transAxes)
    offset = .5
    for (index, sub_items_key) in enumerate([x for x in sorted(data.keys(), key=lambda v: v.replace('d','a').replace('z1','c1').replace('z2','c2').replace('bz','mz')) if on_whitelist(key, x)]):
        labels.append(sub_items_key)
        bar_width = 0.35
        sub_items = data[sub_items_key]
        axen = []
        for (sub_index, sub_item) in enumerate(sub_items):
            kwargs = {}
            if key in do_log:
                kwargs['log'] = True
            #if key not in y_limits:
            #    kwargs['transform'] = trans
            #if sub_index == 0:
            #    kwargs['label'] = key.replace('_', ' ')
            kwargs['color'] = map_color[sub_items_key]
            axen.append(ax.bar(index + offset, sub_item, bar_width, **kwargs))
            rect = axen[-1][-1]
            height = rect.get_height()
            if height > 100:
                dat = '%.0f' %height
            elif height > 20:
                dat = '%.1f' % height
            else:
                dat = '%.2f' % height
            ax.text(rect.get_x() + rect.get_width()/2.0, height, dat, ha='center', va='bottom')
        if index == 0 and len(sub_items) != 1:
            ax.legend(axen, ['p99.99', 'p99', 'p75', 'p50'], ncol=2)

    ax.set_xticks(np.arange(len(labels)) + offset + bar_width * .5)
    ax.set_xticklabels([label_reassign(l) for l in labels])
    ax.set_ylabel(ylabel[key])
    if key in y_limits:                                                                             
        ax.set_ylim(y_limits[key][0], y_limits[key][1])         #
    ax.set_xlim(0,len(labels))
    ax.yaxis.set_major_formatter(ScalarFormatter())
    #ax.set_xticks([offset + x for (x,_) in enumerate(labels)])
                                                                              
def draw(ratio_vs_raw, ratio_vs_zlib, encode_avg, decode_avg, decode_pct):
    rcParams['pdf.fonttype'] = 42
    rcParams['ps.fonttype'] = 42
    rcParams['pgf.rcfonts'] = False
    fig, [ax1, ax2, ax3] = plt.subplots(3, 1, sharex=True, figsize=(6, 6))
    plt.suptitle("Dropbox recent uploads")
    #build_figure('time_pct', ax1, decode_pct, last=True)
    build_figure('decode_speed', ax2, decode_avg, last=True)
    build_figure('encode_speed', ax3, encode_avg)
    build_figure('savings_vs_zlib', ax1, ratio_vs_zlib)
    #fig.subplots_adjust(bottom=0.15, right=.99, top=0.99, hspace=0.03)
    plt.savefig('compression_comparison_ratio_speed_time.pdf')
    plt.savefig('compression_comparison_ratio_speed_time.png')
    fig.clear()

    rcParams['pdf.fonttype'] = 42
    rcParams['ps.fonttype'] = 42
    rcParams['pgf.rcfonts'] = False
    fig, [ax1, ax2] = plt.subplots(2, 1, sharex=True, figsize=(6, 4.5))
    plt.suptitle("Dropbox recent uploads timing")
    #build_figure('time_pct', ax1, decode_pct, last=True)
    build_figure('decode_speed', ax1, decode_avg, last=True)
    build_figure('encode_speed', ax2, encode_avg)
    #fig.subplots_adjust(bottom=0.15, right=.99, top=0.99, hspace=0.03)
    plt.savefig('compression_comparison_speed_time.pdf')
    plt.savefig('compression_comparison_speed_time.png')
    fig.clear()

    rcParams['pdf.fonttype'] = 42
    rcParams['ps.fonttype'] = 42
    rcParams['pgf.rcfonts'] = False
    fig, ax1 = plt.subplots(1, 1, sharex=True, figsize=(6, 2.7))
    plt.suptitle("Dropbox recent uploads compression ratio")
    build_figure('savings_vs_zlib', ax1, ratio_vs_zlib)
    fig.subplots_adjust(bottom=0.2, right=.99, top=.9, hspace=0.03)
    plt.savefig('compression_comparison_ratio.pdf')
    plt.savefig('compression_comparison_ratio.png')
    fig.clear()

