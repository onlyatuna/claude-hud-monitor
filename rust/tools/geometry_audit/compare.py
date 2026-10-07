"""Compares results/py_<mode>.json (PySide6) with results/rs<variant>_<mode>.json (qtrs HUD).

Variants (rust/src/ui/geometry_audit.rs, applied from outside through public widget APIs only):
  0 as is | 1 + Python's widget-local `font-size: 14px` on the metric values | 2 + card root spacing 5
  3 + no Badge max-height (on 2) | 4 no Badge max-height only (on 0)
  5 = 2 + Python's container structure (header policy, body stretch, spacing 8, vertical cards unstretched)
  6 = 5 + cards container / stack policy Preferred | 7 = 6 + no Badge max-height
Usage: python compare.py [results-dir]
"""
import json, sys, os
D = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), 'results')
NAMES = {0: 'as is', 1: '+font 14px', 2: '+spacing 5', 3: '+no badge max-h (on 2)', 4: 'no badge max-h only',
         5: '2 + py structure', 6: '5 + Preferred containers', 7: '6 + no badge max-h'}
KEYS = ['card', 'header', 'm1_box', 'm2_box', 'dot', 'title', 'badge', 'm1_label', 'm1_val', 'm1_bar', 'm1_sub',
        'm2_label', 'm2_val', 'm2_bar', 'm2_sub']
load = lambda f: json.load(open(os.path.join(D, f), encoding='utf-8'))['w']


def rect_eq(a, b, axis):
    return (a[1], a[3]) == (b[1], b[3]) if axis == 'y' else (a[0], a[2]) == (b[0], b[2])


for mode in ('horizontal', 'vertical'):
    py = load(f'py_{mode}.json')
    rs = {v: load(f'rs{v}_{mode}.json') for v in range(8)}
    print(f'\n## {mode}: mismatching widgets per variant (all rects of the three cards + HUD header)')
    print('| variant | y/h | x/w | sizeHint |\n|---|---|---|---|')
    for v in range(8):
        y = x = s = 0
        for k, p in py.items():
            r = rs[v].get(k)
            if not r or 'rect' not in p or 'rect' not in r:
                continue
            y += not rect_eq(p['rect'], r['rect'], 'y')
            x += not rect_eq(p['rect'], r['rect'], 'x')
            s += p['sizeHint'] != r['sizeHint']
        print(f'| V{v} {NAMES[v]} | {y} | {x} | {s} |')
    print(f'\n## {mode}: claude card, as is (V0) vs Python; first variant whose y/h equals Python')
    print('| widget | Python rect | Rust V0 rect | delta | y/h first equal in |\n|---|---|---|---|---|')
    for k in KEYS:
        p = py.get(f'claude.{k}')
        if not p or 'rect' not in p:
            continue
        r = rs[0][f'claude.{k}']['rect']
        d = [b - a for a, b in zip(p['rect'], r)]
        first = next((f'V{v}' for v in range(8) if rect_eq(p['rect'], rs[v][f'claude.{k}']['rect'], 'y')), 'never')
        print(f'| {k} | {p["rect"]} | {r} | {d} | {first} |')
