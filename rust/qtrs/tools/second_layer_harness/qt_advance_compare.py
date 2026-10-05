"""Compares text advances of Qt (PySide6, real Windows platform plugin) with the Rust text layout.

Qt side: QFontMetricsF.horizontalAdvance at the requested device pixel ratio.
Rust side: examples/advance_dump (FontMetrics::horizontal_advance_exact).

Usage: python qt_advance_compare.py [dpr ...]      dpr 1 runs Qt with high-DPI scaling disabled.
Env: ADVANCE_DUMP overrides the advance_dump binary.
"""
import json, os, subprocess, sys

root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
exe = os.environ.get("ADVANCE_DUMP") or os.path.join(root, "target", "release", "examples", "advance_dump.exe")

FAMILIES = [("Microsoft JhengHei UI", [400, 600]), ("Segoe UI", [int(w) for w in os.environ.get("SEGOE_WEIGHTS", "400").split(",")]), ("Microsoft JhengHei", [400])]
SIZES = [10, 12, 14, 15, 16, 18, 20, 24]
TEXTS = [
    "Claude Code", "Codex", "Antigravity", "5 小時", "1 週", "Claude HUD Monitor",
    "AV To Wa Te Yo 11 fi fl", "Typography: AVATAR", "42%", "resets in 2h 13m", "你好，世界",
    "Microsoft JhengHei UI 繁體中文 test", "$1,234.56 — 100%",
]


def cases():
    for fam, weights in FAMILIES:
        for w in weights:
            for px in SIZES:
                for t in TEXTS:
                    yield fam, px, w, t


def qt_widths(dpr):
    code = r'''
import sys, json
from PySide6.QtWidgets import QApplication
from PySide6.QtGui import QFont, QFontMetricsF
app = QApplication(sys.argv[:1])
sys.stderr.write("dpr=%s\n" % app.primaryScreen().devicePixelRatio())
out = []
for line in sys.stdin.read().split("\n"):
    if not line:
        continue
    fam, px, w, text = line.split("\t", 3)
    f = QFont(fam); f.setPixelSize(int(px)); f.setWeight(QFont.Weight(int(w)))
    out.append(QFontMetricsF(f).horizontalAdvance(text))
print(json.dumps(out))
'''
    env = dict(os.environ, QT_SCALE_FACTOR_ROUNDING_POLICY="PassThrough")
    if dpr == 1:
        env["QT_ENABLE_HIGHDPI_SCALING"] = "0"
    else:
        env["QT_SCREEN_SCALE_FACTORS"] = str(dpr)
    stdin = "\n".join(f"{f}\t{p}\t{w}\t{t}" for f, p, w, t in cases())
    r = subprocess.run([sys.executable, "-c", code], input=stdin, capture_output=True, text=True, env=env, check=True)
    return json.loads(r.stdout), r.stderr.strip()


def rust_widths(dpr):
    stdin = "\n".join(f"{f}\t{p}\t{w}\t{t}" for f, p, w, t in cases())
    r = subprocess.run([exe, str(dpr)], input=stdin, capture_output=True, text=True, check=True)
    return [float(x) for x in r.stdout.split()]


for dpr in [float(a) for a in sys.argv[1:]] or [1.0, 1.25, 1.5, 2.0]:
    q, note = qt_widths(dpr)
    rs = rust_widths(dpr)
    rows = list(cases())
    bad = [(rows[i], q[i], rs[i]) for i in range(len(rows)) if abs(q[i] - rs[i]) > 1 / 64 + 1e-6]
    ceil_bad = [i for i in range(len(rows)) if int(-(-q[i] // 1)) != int(-(-rs[i] // 1))]
    worst = max((abs(a - b) for a, b in zip(q, rs)), default=0)
    print(f"dpr {dpr} ({note}): {len(rows)} cases, {len(bad)} differ by > 1/64 px, "
          f"{len(ceil_bad)} differ in ceil (layout width), worst {worst:.4f}")
    for (fam, px, w, t), a, b in sorted(bad, key=lambda r: -abs(r[2] - r[1]))[:int(os.environ.get('SHOW', 12))]:
        print(f"   {fam} {px}px w{w} {t!r}: Qt {a:.4f}  Rust {b:.4f}  d={b - a:+.4f}")
