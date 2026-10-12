"""QLayout::SizeConstraint probe (G9.1.e): how QLayout::activate applies the top-level layout's
size constraint to its owner (qlayout.cpp:980-1133).

Every case builds an owner QWidget (a window, or a child of a shown 800x700 parent) with a
QHBoxLayout (margins 5, spacing 4) over two probe widgets:
  A: sizeHint 60x30, minimumSizeHint 0x0, minimumSize 50x20
  B: sizeHint 40x50, minimumSizeHint 0x0, minimumSize 30x40
so the layout's minimum is 94x50 and its size hint 109x60. The owner is resized, shown and the
layout activated; the line reports the owner size, its minimum and maximum size, and both item
rectangles.

Run: QT_QPA_PLATFORM=offscreen python layout_size_constraint.py
"""

import sys

from PySide6.QtCore import QSize
from PySide6.QtWidgets import QApplication, QHBoxLayout, QLayout, QStackedLayout, QWidget


class Probe(QWidget):
    def __init__(self, parent, hint, minimum):
        super().__init__(parent)
        self._hint = QSize(*hint)
        self.setMinimumSize(*minimum)

    def sizeHint(self):
        return self._hint

    def minimumSizeHint(self):
        return QSize(0, 0)


def build(parent, size, constraint=None, explicit_min=None, max_b=None):
    owner = QWidget(parent)
    layout = QHBoxLayout(owner)
    layout.setContentsMargins(5, 5, 5, 5)
    layout.setSpacing(4)
    a = Probe(owner, (60, 30), (50, 20))
    b = Probe(owner, (40, 50), (30, 40))
    if max_b:
        b.setMaximumSize(*max_b)
    layout.addWidget(a)
    layout.addWidget(b)
    if constraint is not None:
        layout.setSizeConstraint(constraint)
    if explicit_min:
        owner.setMinimumSize(*explicit_min)
    owner.resize(*size)
    owner.show()
    layout.activate()
    return owner, layout, a, b


def fmt(r):
    return f"{r.x()},{r.y()},{r.width()},{r.height()}"


def report(name, owner, a, b):
    mn, mx = owner.minimumSize(), owner.maximumSize()
    print(
        f"{name} size={owner.width()}x{owner.height()} min={mn.width()}x{mn.height()} "
        f"max={mx.width()}x{mx.height()} a={fmt(a.geometry())} b={fmt(b.geometry())}"
    )


def main():
    app = QApplication(sys.argv)
    parent = QWidget()
    parent.resize(800, 700)
    parent.show()
    app.processEvents()

    cases = [
        ("default_window", None, dict(size=(40, 20))),
        ("default_window_explicit_min_w", None, dict(size=(40, 20), explicit_min=(70, 0))),
        ("default_window_explicit_min_wh", None, dict(size=(40, 20), explicit_min=(70, 10))),
        ("default_child", parent, dict(size=(40, 20))),
        ("default_child_explicit_min_w", parent, dict(size=(40, 20), explicit_min=(70, 0))),
        ("fixed_window", None, dict(size=(300, 200), constraint=QLayout.SetFixedSize)),
        ("fixed_child", parent, dict(size=(300, 200), constraint=QLayout.SetFixedSize)),
        ("none_window", None, dict(size=(40, 20), constraint=QLayout.SetNoConstraint)),
        ("minimum_child", parent, dict(size=(40, 20), constraint=QLayout.SetMinimumSize)),
        ("maximum_window", None,
         dict(size=(300, 200), constraint=QLayout.SetMaximumSize, max_b=(45, 55))),
        ("min_and_max_window", None,
         dict(size=(40, 300), constraint=QLayout.SetMinAndMaxSize, max_b=(45, 55))),
    ]
    for name, owner_parent, kw in cases:
        owner, _layout, a, b = build(owner_parent, **kw)
        app.processEvents()
        report(name, owner, a, b)
        owner.close()
        owner.deleteLater()
        app.processEvents()

    # The layout-derived minimum is not explicit: a smaller layout minimum lowers it again
    # (qlayout.cpp:1126-1129 restores the explicit flags), and the window does not shrink.
    owner, layout, a, b = build(None, (40, 20))
    app.processEvents()
    b.setMinimumSize(10, 10)
    layout.activate()
    app.processEvents()
    report("default_window_relaxed", owner, a, b)

    # QStackedLayout's minimum and size hint leave out its contents margins
    # (qstackedlayout.cpp:417-448), so a window grows to the largest page minimum only, and
    # SetFixedSize fixes it to the largest page hint.
    for name, constraint, size in [
        ("stacked_window", None, (10, 10)),
        ("stacked_fixed_window", QLayout.SetFixedSize, (300, 200)),
    ]:
        owner = QWidget()
        stack = QStackedLayout(owner)
        stack.setContentsMargins(8, 8, 8, 8)
        page = Probe(owner, (60, 30), (50, 20))
        stack.addWidget(page)
        if constraint is not None:
            stack.setSizeConstraint(constraint)
        owner.resize(*size)
        owner.show()
        stack.activate()
        app.processEvents()
        mn = owner.minimumSize()
        print(
            f"{name} size={owner.width()}x{owner.height()} min={mn.width()}x{mn.height()} "
            f"page={fmt(page.geometry())}"
        )


if __name__ == "__main__":
    main()
