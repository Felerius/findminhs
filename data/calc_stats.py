#!/usr/bin/python3

import glob
import os.path
import math
from matplotlib import pyplot as plt
import numpy as np

# loads inst as list of edges
def load_inst(inst_name):
    inst = []
    f = open(f'ilps/{inst_name}.lp')
    for line in f:
        if not line.endswith('>= 1\n'):
            continue
        edge = []
        for el in line.split():
            if el.startswith('v'):
                edge.append(int(el[1:]))
        inst.append(edge)
    return inst

def load_solution_size(inst_name):
    f = open(f'results/{inst_name}.sol')
    for line in f:
        return int(list(line.split())[-1])

def degrees(inst):
    deg = {}
    for edge in inst:
        for node in edge:
            deg[node] = deg[node]+1 if node in deg else 1
    return deg

def lower_degree(inst):
    deg = degrees(inst)
    mx = max(deg.values())
    return int(math.ceil(len(inst)/mx))

def lower_packing(inst):
    deg = degrees(inst)
    score = lambda edge: sum(deg[v] for v in edge)
    packing  = []
    for e in sorted(inst, key=score):
        if all( len(set(e) & set(p)) == 0 for p in packing):
            packing.append(e)
    return len(packing)

def lower_sum_degrees(inst):
    deg = degrees(inst)
    edges = len(inst)
    res = 0
    covered = 0
    for d in sorted(deg.values(), reverse=True):
        if covered < edges:
            res += 1
            covered += d
    return res


names = []
opts = []
low_degs = []
low_sums = []
low_paks = []
for path_to_sol in glob.glob("results/*.sol"):
    if len(names)==10:
        break
    inst_name = os.path.basename(path_to_sol)[:-4]
    inst = load_inst(inst_name)
    print('name', inst_name)
    print('\tnodes  ', len(set(v for e in inst for v in e)))
    print('\tedges  ', len(inst))
    opt = load_solution_size(inst_name)
    low_deg = lower_degree(inst)
    low_sum = lower_sum_degrees(inst)
    low_pak = lower_packing(inst)
    print('\tsol    ', opt)
    print('\tlow deg', low_deg)
    print('\tlow sum', low_sum)
    print('\tlow pak', low_pak)
    names.append(inst_name)
    opts.append(opt)
    low_degs.append(low_deg)
    low_sums.append(low_sum)
    low_paks.append(low_pak)
    

#plot stuff
width = 0.2  # the width of the bars

fig, ax = plt.subplots()
x = np.arange(len(names))
ax.bar(x - 1.5*width, low_degs, width, label='deg')
ax.bar(x - 0.5*width, low_sums, width, label='sum')
ax.bar(x + 0.5*width, low_paks, width, label='pak')
ax.bar(x + 1.5*width, opts, width, label='opt')

ax.set_ylabel('bound')
ax.set_title('different lower bounds and optimum per instance')
ax.set_xticks(x)
#ax.set_xticklabels(names, rotation=90)
ax.legend()

#plt.tight_layout()
plt.savefig('plot.pdf')

