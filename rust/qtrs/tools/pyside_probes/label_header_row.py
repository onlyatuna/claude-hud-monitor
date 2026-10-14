"""Real QLabel size hints and layout geometry (G9.7.b, first sub-scope).

One HUD-representative configuration: the header row of the HUD (`hud_window.py:280-288`) reduced to
the two labels whose size is text-driven,

    QHBoxLayout (margins 0, spacing 6): QLabel#HeaderTitle | stretch | QLabel#HeaderStatus

under the HUD's own dark style sheet (`python/ui/styles.py:67-90`, the QLabel rules). No window is
shown; the layout is given a rectangle with `QLayout.setGeometry`.

For each label the line reports sizeHint, minimumSizeHint and minimumSize; for the layout its
sizeHint and minimumSize, and then both item rectangles for a layout rectangle that is exactly the
size hint, 100 wider, and 10 narrower than the layout minimum (height 30 in all three).

Run (Windows, the platform the HUD ships on; PySide6 6.11.2):
  DPR 1.25: set QT_SCREEN_SCALE_FACTORS=1.25
  DPR 1.0 : set QT_ENABLE_HIGHDPI_SCALING=0
  python label_header_row.py
"""

import sys

from PySide6.QtCore import QRect
from PySide6.QtWidgets import QApplication, QHBoxLayout, QLabel, QWidget

SHEET = """
QLabel { color: #e2e8f0; font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif; }
QLabel#HeaderTitle { font-size: 10.5px; font-weight: 800; letter-spacing: 1.0px; color: #94a3b8; }
QLabel#HeaderStatus { font-size: 9.5px; color: #64748b; font-family: 'Consolas', monospace; }
"""

TITLE = "AI AGENT HUD (3-IN-1)"
STATUS = "Updated 12:34:56"


def sz(s):
    return f"{s.width()}x{s.height()}"


def rect(r):
    return f"{r.x()},{r.y()},{r.width()},{r.height()}"


def main():
    app = QApplication(sys.argv)
    app.setStyleSheet(SHEET)

    owner = QWidget()
    layout = QHBoxLayout(owner)
    layout.setContentsMargins(0, 0, 0, 0)
    layout.setSpacing(6)
    title = QLabel(TITLE, owner)
    title.setObjectName("HeaderTitle")
    status = QLabel(STATUS, owner)
    status.setObjectName("HeaderStatus")
    layout.addWidget(title)
    layout.addStretch(1)
    layout.addWidget(status)

    print(f"dpr={app.devicePixelRatio()}")
    for name, w in (("title", title), ("status", status)):
        print(
            f"label {name} hint={sz(w.sizeHint())} minhint={sz(w.minimumSizeHint())} "
            f"min={sz(w.minimumSize())} font={w.font().family()}/{w.font().pixelSize()}"
        )
    hint, minimum = layout.sizeHint(), layout.minimumSize()
    print(f"layout hint={sz(hint)} min={sz(minimum)}")
    for name, width in (
        ("at_hint", hint.width()),
        ("wider", hint.width() + 100),
        ("below_min", minimum.width() - 10),
    ):
        layout.setGeometry(QRect(0, 0, width, 30))
        print(f"case {name} w={width} title={rect(title.geometry())} status={rect(status.geometry())}")


main()
