"""Compares glyph coverage drawn by Qt (PySide6, the Python HUD's text path) with the Rust glyph backend.

Qt side: QPainter on a transparent ARGB32_Premultiplied QImage, scaled by the device pixel ratio like
the real HUD, white text, one glyph per image at a pixel-aligned origin; the alpha channel is the coverage.
Rust side: examples/glyph_dump (uses the active backend, see text::glyph_face::parse_face).

Usage: python qt_glyph_compare.py [dpr=1.25] [sizes=12,13,14,18,24] [family=Segoe UI] [font file]
"""
import json, os, subprocess, sys
import numpy as np
from PySide6.QtGui import QGuiApplication, QFont, QImage, QPainter, QColor
from PySide6.QtCore import QPointF

dpr = float(sys.argv[1]) if len(sys.argv) > 1 else 1.25
sizes = [float(s) for s in (sys.argv[2] if len(sys.argv) > 2 else "12,13,14,18,24").split(",")]
family = sys.argv[3] if len(sys.argv) > 3 else "Segoe UI"
fontfile = sys.argv[4] if len(sys.argv) > 4 else r"C:\Windows\Fonts\segoeui.ttf"
CHARS = os.environ.get("GLYPH_CHARS", "CHUDMonitr5h42%7d18:$.,0123456789gyQ@&AIlo")

app = QGuiApplication(sys.argv[:1])
print(f"Qt devicePixelRatio={app.devicePixelRatio()}  (requested {dpr})")
root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
exe = os.environ.get("GLYPH_DUMP") or os.path.join(root, "target", "release", "examples", "glyph_dump.exe")

W, H = 96, 96
OX, OY = 12, 60  # physical pixel origin of the glyph (pen position on the baseline)


def qt_alpha(ch, size):
    img = QImage(W, H, QImage.Format_ARGB32_Premultiplied)
    img.fill(0)
    p = QPainter(img)
    p.setRenderHint(QPainter.TextAntialiasing, True)
    p.scale(dpr, dpr)
    f = QFont(family)
    f.setPixelSize(int(size))
    p.setFont(f)
    p.setPen(QColor(255, 255, 255, 255))
    p.drawText(QPointF(OX / dpr, OY / dpr), ch)
    p.end()
    ptr = img.constBits()
    arr = np.frombuffer(ptr, dtype=np.uint8).reshape(H, img.bytesPerLine() // 4, 4)[:, :W, :]
    return arr[:, :, 3].astype(np.int32)  # BGRA byte order -> alpha is byte 3


tot = {"px": 0, "sum": 0, "n": 0, "exact": 0, "bbox_mismatch": 0, "worst": 0}
rows = []
for size in sizes:
    out = subprocess.run([exe, fontfile, str(size), str(dpr), CHARS], capture_output=True, text=True, check=True).stdout
    s_sum = s_px = s_bbox = s_n = s_exact = 0
    s_worst = 0
    for line in out.strip().splitlines():
        g = json.loads(line)
        ch = chr(g["cp"])
        q = qt_alpha(ch, size)
        r = np.zeros((H, W), dtype=np.int32)
        if g["w"] and g["h"]:
            bits = np.frombuffer(bytes.fromhex(g["bits"]), dtype=np.uint8).astype(np.int32).reshape(g["h"], g["w"])
            x0 = OX + g["xmin"]
            y0 = OY - g["ymin"] - g["h"]
            ys, xs = max(y0, 0), max(x0, 0)
            ye, xe = min(y0 + g["h"], H), min(x0 + g["w"], W)
            r[ys:ye, xs:xe] = bits[ys - y0:ye - y0, xs - x0:xe - x0]
        qb = np.argwhere(q > 0)
        rb = np.argwhere(r > 0)
        bbox = lambda a: None if len(a) == 0 else (a.min(0).tolist(), a.max(0).tolist())
        if bbox(qb) != bbox(rb):
            s_bbox += 1
        d = np.abs(q - r)
        s_sum += int(d.sum())
        s_px += int(((q > 0) | (r > 0)).sum())
        s_worst = max(s_worst, int(d.max()))
        s_n += 1
        s_exact += int(d.max() == 0)
    rows.append((size, s_n, s_exact, s_bbox, s_sum / max(s_px, 1), s_worst))
    for k, v in (("px", s_px), ("sum", s_sum), ("n", s_n), ("exact", s_exact), ("bbox_mismatch", s_bbox)):
        tot[k] += v
    tot["worst"] = max(tot["worst"], s_worst)

print(f"{'size':>5} {'glyphs':>6} {'identical':>9} {'bbox differs':>12} {'mean |diff| over ink px':>24} {'max':>4}")
for size, n, ex, bb, mean, worst in rows:
    print(f"{size:5.0f} {n:6d} {ex:9d} {bb:12d} {mean:24.2f} {worst:4d}")
print(f"TOTAL glyphs {tot['n']}, identical {tot['exact']}, bbox differs {tot['bbox_mismatch']}, mean |diff| over ink px {tot['sum'] / max(tot['px'], 1):.2f}/255, max {tot['worst']}")
