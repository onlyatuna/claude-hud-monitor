"""PySide6 oracle for QGridLayout::minimumHeightForWidth (qgridlayout.cpp:374-382).

A two-row grid with 0 margins and 0 spacing:
- Row 0: widget with sizeHint (40, 50) and minimumSize (40, 20)
- Row 1: HFW widget with heightForWidth(w) = 2 * w

For width 40:
- heightForWidth(40) = 50 + 80 = 130
- minimumHeightForWidth(40) = 20 + 80 = 100

Run: QT_QPA_PLATFORM=offscreen python grid_minimum_height_for_width.py
"""

import sys
from PySide6.QtCore import QSize
from PySide6.QtWidgets import QApplication, QGridLayout, QWidget


class Row0Widget(QWidget):
    def sizeHint(self):
        return QSize(40, 50)

    def minimumSizeHint(self):
        return QSize(40, 20)

    def minimumSize(self):
        return QSize(40, 20)


class Row1Widget(QWidget):
    def __init__(self):
        super().__init__()
        sp = self.sizePolicy()
        sp.setHeightForWidth(True)
        self.setSizePolicy(sp)

    def sizeHint(self):
        return QSize(40, 20)

    def hasHeightForWidth(self):
        return True

    def heightForWidth(self, w):
        return 2 * w


def main():
    app = QApplication.instance() or QApplication(sys.argv)
    grid = QGridLayout()
    grid.setContentsMargins(0, 0, 0, 0)
    grid.setSpacing(0)
    r0 = Row0Widget()
    r1 = Row1Widget()
    grid.addWidget(r0, 0, 0)
    grid.addWidget(r1, 1, 0)

    parent = QWidget()
    parent.setLayout(grid)

    print(f"hfw={grid.heightForWidth(40)} min_hfw={grid.minimumHeightForWidth(40)}")


if __name__ == "__main__":
    main()
