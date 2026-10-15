"""Qt oracle for the `:hover` paint of the real Python `LayoutToggleBtn` (G9.7.b sub-scope 5).

Real `QPushButton("⇄")` named `LayoutToggleBtn`, styled by the real `get_cards_stylesheet(False)` (the
cards-mode sheet: its `:hover` rule changes the background, the text colour and the border colour; the
table-mode sheet hides this button, so its `:hover` never shows). The button is a child of a 60x40
widget at (11, 11), 28x18, so the layout is out of the picture and only the paint is compared.

Run on the `offscreen` platform: `QTest.mouseMove` really moves the (virtual) cursor there, so
`underMouse()` and the `:hover` pseudo state are the real ones; on the Windows platform with a window
that is not on screen the cursor never enters the button. The cursor is first moved outside the button
(normal), then onto it (hover), and the run is repeated once to prove the pixels are stable.

Prints, for each state, the `grab()` as straight (non-premultiplied) ARGB32 hex, one row per line.
Usage (DPR 1.0 and 1.25):
  QT_QPA_PLATFORM=offscreen python button_hover.py
  QT_QPA_PLATFORM=offscreen QT_SCREEN_SCALE_FACTORS=1.25 python button_hover.py
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "python"))

from PySide6.QtCore import QPoint
from PySide6.QtGui import QImage
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication, QPushButton, QWidget

from ui.styles import get_cards_stylesheet

app = QApplication([])
app.setStyleSheet(get_cards_stylesheet(False))
owner = QWidget()
owner.resize(60, 40)
btn = QPushButton("⇄", owner)
btn.setObjectName("LayoutToggleBtn")
btn.setGeometry(11, 11, 28, 18)
owner.show()
QTest.qWait(50)


def grab(expect_under):
    assert btn.underMouse() == expect_under, f"underMouse {btn.underMouse()} != {expect_under}"
    img = btn.grab().toImage().convertToFormat(QImage.Format_ARGB32)
    rows = [" ".join("%08x" % img.pixel(x, y) for x in range(img.width())) for y in range(img.height())]
    return img.devicePixelRatio(), img.width(), img.height(), rows


results = {}
for rep in range(2):
    QTest.mouseMove(owner, QPoint(55, 35))
    QTest.qWait(50)
    results[("normal", rep)] = grab(False)
    QTest.mouseMove(owner, QPoint(25, 20))
    QTest.qWait(50)
    results[("hover", rep)] = grab(True)
assert results[("normal", 0)] == results[("normal", 1)] and results[("hover", 0)] == results[("hover", 1)], "unstable"
for state in ("normal", "hover"):
    dpr, w, h, rows = results[(state, 0)]
    print(f"{state} dpr={dpr} size={w}x{h}")
    print("\n".join(rows))
