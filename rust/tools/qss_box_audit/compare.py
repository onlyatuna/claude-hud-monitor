"""Compares results/py_box[_dpr1].json (PySide6) with results/rs_box[_dpr1].json (qtrs). Usage: python compare.py"""
import json, os
D = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'results')
for suffix in ('', '_dpr1'):
    py = json.load(open(os.path.join(D, f'py_box{suffix}.json'))); rs = json.load(open(os.path.join(D, f'rs_box{suffix}.json')))
    print(f"\n## DPR {py['meta']['dpr']}   (differences only; P = PySide6, R = qtrs)")
    n = {'min': 0, 'max': 0, 'hint': 0, 'minHint': 0}
    for k in py:
        if k == 'meta': continue
        row = []
        for f in ('min', 'max', 'hint', 'minHint'):
            if py[k][f] != rs[k][f]:
                n[f] += 1; row.append(f"{f}: P{py[k][f]} R{rs[k][f]}")
        print(f"{k:3} {'OK' if not row else '; '.join(row)}")
    print('mismatch counts', n)
