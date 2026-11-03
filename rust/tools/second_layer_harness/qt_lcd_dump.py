"""Dump Qt (PySide6, windows platform) text drawn onto several destinations as raw RGBA bins.

usage: lcd_q.py <family> <pixel size> <text> <out dir>
Each <case>.bin is W*H*4 bytes, RGBA straight (QImage.pixelColor) so the test needs no premultiply.
"""
import sys, os
from PySide6.QtWidgets import QApplication
from PySide6.QtGui import QFont, QImage, QPainter, QColor

app = QApplication([])
fam, px, text, outdir = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
os.makedirs(outdir, exist_ok=True)
f = QFont(fam); f.setPixelSize(px)
W, H, BASE_X, BASE_Y = 48, 24, 2, 17
BG = QColor(22, 25, 32, 255)
FG = QColor(226, 232, 240, 255)
cases = {
    "rgb32_opaque": (QImage.Format_RGB32, BG, FG),
    "argb_pre_opaque": (QImage.Format_ARGB32_Premultiplied, BG, FG),
    "argb_pre_clear": (QImage.Format_ARGB32_Premultiplied, QColor(0, 0, 0, 0), FG),
    "argb_pre_half": (QImage.Format_ARGB32_Premultiplied, QColor(22, 25, 32, 128), FG),
    "argb_straight": (QImage.Format_ARGB32, BG, FG),
    "rgb32_textalpha": (QImage.Format_RGB32, BG, QColor(226, 232, 240, 128)),
    "rgb32_black_on_white": (QImage.Format_RGB32, QColor(255, 255, 255), QColor(0, 0, 0)),
}
for name, (fmt, bg, fg) in cases.items():
    im = QImage(W, H, fmt)
    im.fill(bg)
    p = QPainter(im)
    p.setFont(f); p.setPen(fg)
    p.drawText(BASE_X, BASE_Y, text)
    p.end()
    buf = bytearray()
    for y in range(H):
        for x in range(W):
            c = im.pixelColor(x, y)
            buf += bytes((c.red(), c.green(), c.blue(), c.alpha()))
    with open(os.path.join(outdir, name + ".bin"), "wb") as fh:
        fh.write(buf)
print("ok", fam, px, repr(text))
