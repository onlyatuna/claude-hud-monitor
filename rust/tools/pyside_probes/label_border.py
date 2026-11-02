"""Qt oracle for the QSS background + border paint of a real `QLabel` (RC-210, Label stage).

Cases (all empty-text labels, so no glyph pixels: only the rule's background and border are painted):
  badge     the real `QLabel#Badge` of the real `get_cards_stylesheet(False)` (40x15; radius 3px,
            `border: 1px solid rgba(255,255,255,0.08)`, background rgba(255,255,255,0.06))
  r4_clear  `background: transparent; border: 1px solid rgba(255,255,255,0.12); border-radius: 4px` 28x18
            (the `LayoutToggleBtn` rule on a QLabel)
  r0        `background: #204060; border: 1px solid #ff8040` 20x12, no radius
  border_only  `border: 1px solid #ff8040; border-radius: 3px` 20x12, no background
  b2        `background: #204060; border: 2px solid #ff8040; border-radius: 4px` 24x16

Each label is rendered with `QWidget.render` onto a transparent ARGB32 image whose device pixel ratio is
the screen's, so the output is the label's own paint (straight, non-premultiplied ARGB32 hex rows), with
no window background in the way.
Usage (DPR 1.0 and 1.25):
  QT_QPA_PLATFORM=offscreen python label_border.py
  QT_QPA_PLATFORM=offscreen QT_SCREEN_SCALE_FACTORS=1.25 python label_border.py
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "python"))

from PySide6.QtCore import Qt
from PySide6.QtGui import QImage
from PySide6.QtWidgets import QApplication, QLabel, QWidget

from ui.styles import get_cards_stylesheet

app = QApplication([])
dpr = app.primaryScreen().devicePixelRatio()
extra = """
QLabel#R4Clear { background-color: transparent; border: 1px solid rgba(255, 255, 255, 0.12); border-radius: 4px; }
QLabel#R0 { background-color: #204060; border: 1px solid #ff8040; }
QLabel#BorderOnly { border: 1px solid #ff8040; border-radius: 3px; }
QLabel#B2 { background-color: #204060; border: 2px solid #ff8040; border-radius: 4px; }
"""
app.setStyleSheet(get_cards_stylesheet(False) + extra)

CASES = [
    ("badge", "Badge", 40, 15),
    ("r4_clear", "R4Clear", 28, 18),
    ("r0", "R0", 20, 12),
    ("border_only", "BorderOnly", 20, 12),
    ("b2", "B2", 24, 16),
]
for name, obj, w, h in CASES:
    label = QLabel("")
    label.setObjectName(obj)
    label.resize(w, h)
    label.ensurePolished()
    img = QImage(round(w * dpr), round(h * dpr), QImage.Format_ARGB32_Premultiplied)
    img.setDevicePixelRatio(dpr)
    img.fill(Qt.transparent)
    label.render(img, renderFlags=QWidget.RenderFlag.DrawChildren)  # no DrawWindowBackground
    img = img.convertToFormat(QImage.Format_ARGB32)
    print(f"{name} dpr={dpr} size={img.width()}x{img.height()}")
    for y in range(img.height()):
        print(" ".join("%08x" % img.pixel(x, y) for x in range(img.width())))
