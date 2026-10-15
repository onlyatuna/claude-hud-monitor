"""HUD header row under show/hide of its optional items (G9.7.b, sub-scope 2).

The header of `hud_window.py:278-288`, with the real HUD widgets and the real cards style sheet
(`get_cards_stylesheet(vibrant=True, dark=True)`, the one the Rust oracle tests install):

    QVBoxLayout (margins 0, spacing 6) > QHBoxLayout (spacing 6, nested):
        status_dot | title | ghost_label | layout_toggle_btn | stretch | time

The layout is 256 wide (the vertical HUD's 280 minus its margins 12 + 12) and 86 high (the header's
height in the 463-high vertical HUD, `hud_window.rs` test `test_vertical_extra_height_goes_to_header`).
No window is shown; the owner is resized to the rectangle and the outer layout activated.

Steps, in order, each printing the rectangle of every visible item (hidden items print `-`):
  start         ghost hidden, toggle visible (the HUD's initial state)
  ghost_on      ghost shown   (click-through on, `hud_window.rs:741`)
  toggle_off    toggle hidden (table mode,        `:809`)
  ghost_off     ghost hidden
  toggle_on     toggle shown  (cards mode,        `:830`)
  both_off      both hidden
Each step calls `setVisible`, then the outer `invalidate()` and `activate()` and prints the result.
(Qt's `QLayout::activate` invalidates the nested layout; see above.)

Run (Windows; PySide6 6.11.2): DPR 1.25 -> QT_SCREEN_SCALE_FACTORS=1.25; DPR 1.0 -> QT_ENABLE_HIGHDPI_SCALING=0
  python header_row_visibility.py
"""

import sys
from pathlib import Path

PY_ROOT = str(Path(__file__).resolve().parents[4] / "python")
sys.path.insert(0, PY_ROOT)

from PySide6.QtCore import QRect
from PySide6.QtWidgets import QApplication, QHBoxLayout, QLabel, QPushButton, QVBoxLayout, QWidget

from ui.styles import get_cards_stylesheet

RECT = QRect(0, 0, 256, 86)


def rect(w):
    if w.isHidden():
        return "-"
    g = w.geometry()
    return f"{g.x()},{g.y()},{g.width()},{g.height()}"


def main():
    app = QApplication(sys.argv)
    app.setStyleSheet(get_cards_stylesheet(True, dark=True))

    owner = QWidget()
    status_dot = QLabel("●", owner)
    status_dot.setStyleSheet("color: #10b981; font-size: 11px;")
    title = QLabel("AI AGENT HUD (3-IN-1)", owner)
    title.setObjectName("HeaderTitle")
    ghost = QLabel("\U0001F47B", owner)
    # The ghost label has no font-size in the sheet, so its hint comes from the default font size:
    # Qt's own hint is 17x16 at DPR 1.25 and 21x20 at 1.0 (Segoe UI 9pt), qtrs's is 18x18 and 18x17
    # (a fixed 13px default). That font difference is a separate root cause (see the Contract's
    # G9.7.b note); the ghost is fixed at 18x18 here, on both sides, to compare the layout alone.
    ghost.setFixedSize(18, 18)
    ghost.setVisible(False)
    toggle = QPushButton("⇄", owner)
    toggle.setObjectName("LayoutToggleBtn")
    time = QLabel("--:--:--", owner)
    time.setObjectName("HeaderStatus")

    # As in the HUD, the header is a nested layout of the window's vertical layout
    # (`inner_layout.addLayout(header_layout)`, `hud_window.py:288`).
    layout = QHBoxLayout()
    layout.setSpacing(6)
    for w in (status_dot, title, ghost, toggle):
        layout.addWidget(w)
    layout.addStretch()
    layout.addWidget(time)
    root = QVBoxLayout(owner)
    root.setContentsMargins(0, 0, 0, 0)
    root.setSpacing(6)
    root.addLayout(layout)

    items = (("dot", status_dot), ("title", title), ("ghost", ghost), ("toggle", toggle), ("time", time))

    def line(step):
        cells = " ".join(f"{n}={rect(w)}" for n, w in items)
        print(f"{step} {cells}")

    print(f"dpr={app.devicePixelRatio()}")
    print(f"hint dot={status_dot.sizeHint().width()}x{status_dot.sizeHint().height()} "
          f"title={title.sizeHint().width()}x{title.sizeHint().height()} "
          f"ghost={ghost.sizeHint().width()}x{ghost.sizeHint().height()} "
          f"toggle={toggle.sizeHint().width()}x{toggle.sizeHint().height()} "
          f"time={time.sizeHint().width()}x{time.sizeHint().height()}")
    # The HUD window has an explicit minimum width (250 in vertical mode, `hud_window.rs`
    # MIN_VERTICAL_WIDTH) that wins over the layout's own minimum (264 with every item shown), so the
    # window stays 280 wide and the header is squeezed. Without it Qt would grow the top-level owner.
    owner.setMinimumSize(250, 0)
    owner.resize(RECT.size())
    root.activate()
    line("start")
    steps = (
        ("ghost_on", ghost, True),
        ("toggle_off", toggle, False),
        ("ghost_off", ghost, False),
        ("toggle_on", toggle, True),
    )
    for name, widget, visible in steps:
        widget.setVisible(visible)
        root.invalidate()
        root.activate()
        line(name)
    ghost.setVisible(False)
    toggle.setVisible(False)
    root.invalidate()
    root.activate()
    line("both_off")


main()
