
import matplotlib.pyplot as plt 
from matplotlib import rcParams                                                                     
                          
import matplotlib.patches as patches
import matplotlib.transforms as transforms                                                                          
import numpy as np
from matplotlib.ticker import ScalarFormatter
def on_whitelist(key, label):
    #if 'key' == 'time_pct':
    #    return label in ('b11, d0')
    return label in ('b11', 'b9', 'd0', 'd4', 'zlib', 'z19')
def label_reassign(key):
    keymap = {
        'b11': 'brotli-11',
        'b9': 'brotli-9',
        'd0': 'divans-11',
        'd4': 'divans',
        'z19': 'zstd',
        }
    if key in keymap:
        return keymap[key]
    return key
colors = [[r for r in reversed(['#aaaaff','#8888cc','#4444aa','#000088',])],                        
           [r for r in reversed(['#ffaaaa','#cc8888','#aa4444','#880000',])]]                       
ylabel = {
    'savings_vs_zlib':'% saving vs zlib\n',
    'encode_speed': 'Encode (Mbps)',
    'decode_speed': 'Decode (Mbps)',
    'time_pct':'Decode Time (ms)',
    }

y_limits= {
    #'savings_vs_zlib':[-.001, 28],
    'encode_speed': [1,100],
    'decode_speed': [10,2000],
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
    for (index, sub_items_key) in enumerate([x for x in sorted(data.keys(), key=lambda v: v.replace('d','a')) if on_whitelist(key, x)]):
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
            if len(sub_items) != 1:
                kwargs['color'] = colors[0][sub_index]
            else:
                kwargs['color'] = colors[0][-1]
            axen.append(ax.bar(index + offset, sub_item, bar_width, **kwargs))
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
    #build_figure('time_pct', ax1, decode_pct, last=True)
    build_figure('decode_speed', ax1, decode_avg, last=True)
    build_figure('encode_speed', ax2, encode_avg)
    build_figure('savings_vs_zlib', ax3, ratio_vs_zlib)
    #fig.subplots_adjust(bottom=0.15, right=.99, top=0.99, hspace=0.03)
    plt.savefig('compression_comparison_ratio_speed_time.pdf')
    fig.clear()

