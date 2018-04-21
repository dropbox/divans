
import matplotlib.pyplot as plt 
from matplotlib import rcParams                                                                     
                                                                                                    
import numpy as np
def on_whitelist(key, label):
    #if 'key' == 'time_pct':
    #    return label in ('b11, d0')
    return label in ('b11', 'b9', 'd0', 'd6', 'zlib', 'z22')
colors = [[r for r in reversed(['#aaaaff','#8888cc','#4444aa','#000088',])],                        
           [r for r in reversed(['#ffaaaa','#cc8888','#aa4444','#880000',])]]                       
ylabel = {
    'savings_vs_zlib':'% savings',
    'encode_speed': 'Mbps',
    'decode_speed': 'Mbps',
    'time_pct':'ms',
    }

y_limits= {
    'savings_vs_zlib':[.001, 15],
    'encode_speed': [0,100],
    'decode_speed': [0,100],
#    'time_pct':
    }
do_log = set()#set(['encode_speed', 'decode_speed'])
def build_figure(key, ax, data, last=False):
    if key in do_log:
        ax.set_yscale('log')
    else:
        ax.set_yscale('linear')
    labels = []
    offset = .5
    for (index, sub_items_key) in enumerate([x for x in sorted(data.keys(), key=lambda v: v.replace('d','a')) if on_whitelist(key, x)]):
        labels.append(sub_items_key)
        bar_width = 0.35
        sub_items = data[sub_items_key]
        for (sub_index, sub_item) in enumerate(sub_items):
            kwargs = {}
            if key in do_log:
                kwargs['log'] = True
            #if sub_index == 0:
            #    kwargs['label'] = key.replace('_', ' ')
            if len(sub_items) != 1:
                kwargs['color'] = colors[0][sub_index]
            else:
                kwargs['color'] = colors[0][-1]
            ax.bar(index + offset, sub_item, bar_width, **kwargs)
    ax.set_xticks(np.arange(len(labels)) + offset + bar_width * .5)
    ax.set_xticklabels(labels)
    ax.set_ylabel(ylabel[key])
    if key in y_limits:                                                                             
        ax.set_ylim(y_limits[key][0], y_limits[key][1])         #
    ax.set_xlim(0,len(labels))
    #ax.set_xticks([offset + x for (x,_) in enumerate(labels)])
                                                                              
def draw(ratio_vs_raw, ratio_vs_zlib, encode_avg, decode_avg, decode_pct):
    rcParams['pdf.fonttype'] = 42
    rcParams['ps.fonttype'] = 42
    rcParams['pgf.rcfonts'] = False

    fig, [ax1, ax2, ax3, ax4] = plt.subplots(4, 1, sharex=True, figsize=(6, 6))
    build_figure('savings_vs_zlib', ax1, ratio_vs_zlib)
    build_figure('encode_speed', ax2, encode_avg)
    build_figure('decode_speed', ax3, decode_avg)
    build_figure('time_pct', ax4, decode_pct, last=True)
    #fig.subplots_adjust(bottom=0.15, right=.99, top=0.99, hspace=0.03)
    plt.savefig('compression_comparison_ratio_speed_time.pdf')
    fig.clear()

