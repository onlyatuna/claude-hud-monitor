"""G9.2.d alignment residual probe: one line per case.

(1) an aligned item with heightForWidth takes heightForWidth(width) (qlayoutitem.cpp:441-447);
(2) a layout's own alignment (QLayout::alignmentRect, qlayout.cpp:1410-1455; qboxlayout.cpp:743,
    qgridlayout.cpp:1324);
(3) right-to-left: QStyle::visualAlignment flips Left/Right unless AlignAbsolute
    (qguiapplication_p.h:176-186); a QLabel decides on its *text* direction (qlabel.cpp:996,
    1456-1464), so Latin text does not flip;
(4) QLayout::setAlignment(QLayout*, align) on a nested layout (qlayout.cpp:213-227);
(5) Qt has one Qt::Alignment for layout items, QLabel, QTextBlockFormat and QProgressBar; QLabel
    honours its vertical flags (qlabel.cpp:1067-1070 via drawItemText / qt_format_text, 1632-1635)
    and adds the frame indent on the Top/Bottom side (qlabel.cpp:558-568, 1474-1488).

Leaves are Fixed 40x20 widgets; containers are 200x100 with margins 0 unless noted. Rects are in
container coordinates.

Run: QT_QPA_PLATFORM=offscreen python layout_alignment_residual.py
"""

import sys

from PySide6.QtCore import QPoint, QSize, Qt
from PySide6.QtGui import QFont, QFontMetrics, QImage, QRegion, QTextBlockFormat, QTextDocument
from PySide6.QtWidgets import (
    QApplication,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QProgressBar,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

A = Qt.AlignmentFlag


class Fixed(QWidget):
    def __init__(self, w=40, h=20):
        super().__init__()
        self._hint = QSize(w, h)
        self.setSizePolicy(QSizePolicy.Fixed, QSizePolicy.Fixed)

    def sizeHint(self):
        return self._hint

    def minimumSizeHint(self):
        return QSize(0, 0)


class Hfw(QWidget):
    """sizeHint 40x20, Preferred/Preferred with heightForWidth(w) = 1600 / w."""

    def __init__(self):
        super().__init__()
        sp = QSizePolicy(QSizePolicy.Preferred, QSizePolicy.Preferred)
        sp.setHeightForWidth(True)
        self.setSizePolicy(sp)

    def sizeHint(self):
        return QSize(40, 20)

    def minimumSizeHint(self):
        return QSize(0, 0)

    def heightForWidth(self, w):
        return 1600 // max(w, 1)


def r(w):
    g = w.geometry()
    return f"({g.x()},{g.y()},{g.width()},{g.height()})"


def contained(layout, w=200, h=100):
    c = QWidget()
    c.setLayout(layout)
    c.resize(w, h)
    c.show()
    layout.activate()
    QApplication.processEvents()
    return c


def leaf_in_container(leaf, c):
    p = leaf.mapTo(c, leaf.rect().topLeft())
    return f"({p.x()},{p.y()},{leaf.width()},{leaf.height()})"


def box(cls):
    b = cls()
    b.setContentsMargins(0, 0, 0, 0)
    b.setSpacing(0)
    return b


def grid():
    g = QGridLayout()
    g.setContentsMargins(0, 0, 0, 0)
    g.setSpacing(0)
    return g


def out(name, value):
    print(f"{name} {value}")


def item1():
    h = box(QHBoxLayout)
    w = Hfw()
    h.addWidget(w, 0, A.AlignVCenter)
    _c = contained(h)
    out("item1_hbox_vcenter_hfw", r(w))
    g = grid()
    w2 = Hfw()
    g.addWidget(w2, 0, 0, A.AlignLeft | A.AlignVCenter)
    _c2 = contained(g)
    out("item1_grid_left_vcenter_hfw", r(w2))
    v = box(QVBoxLayout)
    w3 = Hfw()
    v.addWidget(w3, 0, A.AlignTop)
    _c3 = contained(v)
    out("item1_vbox_top_hfw", r(w3))


def item2():
    v = box(QVBoxLayout)
    leaf = Fixed()
    v.addWidget(leaf)
    v.setAlignment(A.AlignRight | A.AlignBottom)
    c = contained(v)
    m = v.maximumSize()
    out("item2_vbox_layout_right_bottom", f"{r(leaf)} max=({m.width()},{m.height()})")
    g = grid()
    leaf2 = Fixed()
    g.addWidget(leaf2, 0, 0)
    g.setAlignment(A.AlignCenter)
    _c2 = contained(g)
    m2 = g.maximumSize()
    out("item2_grid_layout_center", f"{r(leaf2)} max=({m2.width()},{m2.height()})")
    v3 = box(QVBoxLayout)
    w3 = Hfw()
    v3.addWidget(w3)
    v3.setAlignment(A.AlignTop)
    _c3 = contained(v3)
    out("item2_vbox_layout_top_hfw", r(w3))
    del c


def item3():
    QApplication.setLayoutDirection(Qt.RightToLeft)
    try:
        h = box(QHBoxLayout)
        leaf = Fixed()
        h.addWidget(leaf)
        h.setAlignment(A.AlignLeft)
        _c = contained(h)
        out("item3_rtl_hbox_layout_left", r(leaf))
        h2 = box(QHBoxLayout)
        leaf2 = Fixed()
        h2.addWidget(leaf2)
        h2.setAlignment(A.AlignLeft | A.AlignAbsolute)
        _c2 = contained(h2)
        out("item3_rtl_hbox_layout_left_absolute", r(leaf2))
        g = grid()
        leaf3 = Fixed()
        g.addWidget(leaf3, 0, 0, A.AlignRight | A.AlignTop)
        _c3 = contained(g)
        out("item3_rtl_grid_item_right_top", r(leaf3))
        g4 = grid()
        leaf4 = Fixed()
        g4.addWidget(leaf4, 0, 0, A.AlignRight | A.AlignAbsolute | A.AlignTop)
        _c4 = contained(g4)
        out("item3_rtl_grid_item_right_absolute_top", r(leaf4))
        lab = QLabel("Hg")
        lab.setFont(label_font())
        lab.resize(200, 40)
        out("item3_rtl_label_left_ink", f"{ink(lab)} layoutDirection={int(lab.layoutDirection().value)}")
    finally:
        QApplication.setLayoutDirection(Qt.LeftToRight)


def item4():
    outer = box(QHBoxLayout)
    inner = box(QVBoxLayout)
    leaf = Fixed()
    inner.addWidget(leaf)
    outer.addLayout(inner)
    ok = outer.setAlignment(inner, A.AlignRight | A.AlignBottom)
    c = contained(outer)
    out("item4_box_set_alignment_nested", f"ok={ok} {leaf_in_container(leaf, c)} inner_align={int(inner.alignment().value)}")
    g = grid()
    inner2 = box(QVBoxLayout)
    leaf2 = Fixed()
    inner2.addWidget(leaf2)
    g.addLayout(inner2, 0, 0, A.AlignCenter)
    c2 = contained(g)
    out("item4_grid_add_layout_center", f"{leaf_in_container(leaf2, c2)} inner_align={int(inner2.alignment().value)}")


def label_font():
    f = QFont("Arial")
    f.setPixelSize(12)
    return f


def ink(label):
    """Bounding box of the painted (non-transparent) pixels: (left, top, right, bottom), inclusive."""
    img = QImage(label.size(), QImage.Format_ARGB32_Premultiplied)
    img.fill(0)
    label.render(img, QPoint(), QRegion(), QWidget.RenderFlag.DrawChildren)
    xs, ys = [], []
    for y in range(img.height()):
        for x in range(img.width()):
            if (img.pixel(x, y) >> 24) & 0xFF:
                xs.append(x)
                ys.append(y)
    if not xs:
        return "(empty)"
    return f"({min(xs)},{min(ys)},{max(xs)},{max(ys)})"


def item5():
    def type_name(value):
        return type(value).__name__

    lab = QLabel("Hg")
    pb = QProgressBar()
    doc = QTextDocument()
    doc.setHtml('<p align="center">x</p><p>y</p>')
    blocks = [doc.begin(), doc.begin().next()]
    h = box(QHBoxLayout)
    leaf = Fixed()
    h.addWidget(leaf, 0, A.AlignHCenter)
    out(
        "item5_types",
        f"label={type_name(lab.alignment())} item={type_name(h.itemAt(0).alignment())} "
        f"block={type_name(blocks[0].blockFormat().alignment())} progress={type_name(pb.alignment())}",
    )
    out(
        "item5_defaults",
        f"label={int(lab.alignment().value)} progress={int(pb.alignment().value)} "
        f"block_unset={int(QTextBlockFormat().alignment().value)} html_center={int(blocks[0].blockFormat().alignment().value)} "
        f"html_plain={int(blocks[1].blockFormat().alignment().value)} center={int(A.AlignCenter.value)}",
    )
    f = label_font()
    fm = QFontMetrics(f)
    for name, align in [
        ("left_top", A.AlignLeft | A.AlignTop),
        ("left_vcenter", A.AlignLeft | A.AlignVCenter),
        ("left_bottom", A.AlignLeft | A.AlignBottom),
        ("left_only", A.AlignLeft),
        ("center", A.AlignCenter),
        ("vcenter_only", A.AlignVCenter),
        ("right_bottom", A.AlignRight | A.AlignBottom),
    ]:
        l = QLabel("Hg")
        l.setFont(f)
        l.setAlignment(align)
        l.resize(200, 60)
        out(f"item5_label_ink_{name}", f"{ink(l)} fm_height={fm.height()}")
    # Frame indent: a frame (here a style-sheet border) adds 'x' advance on each aligned side.
    for name, align in [
        ("left_vcenter", A.AlignLeft | A.AlignVCenter),
        ("hcenter_vcenter", A.AlignHCenter | A.AlignVCenter),
        ("hcenter_top", A.AlignHCenter | A.AlignTop),
        ("left_top", A.AlignLeft | A.AlignTop),
        ("hcenter_bottom", A.AlignHCenter | A.AlignBottom),
    ]:
        l = QLabel("Hg")
        l.setFont(f)
        l.setStyleSheet("QLabel { border: 1px solid black; }")
        l.setAlignment(align)
        s = l.sizeHint()
        out(f"item5_framed_hint_{name}", f"({s.width()},{s.height()}) x_adv={fm.horizontalAdvance('x')}")
    # The same indent insets the drawn text by half on the aligned sides (qlabel.cpp:1474-1488);
    # a transparent border keeps the frame but paints nothing.
    for name, align in [
        ("left_top", A.AlignLeft | A.AlignTop),
        ("left_vcenter", A.AlignLeft | A.AlignVCenter),
        ("right_bottom", A.AlignRight | A.AlignBottom),
    ]:
        l = QLabel("Hg")
        l.setFont(f)
        l.setStyleSheet("QLabel { border: 1px solid transparent; }")
        l.setAlignment(align)
        l.resize(200, 60)
        out(f"item5_framed_ink_{name}", ink(l))
    # Rich text label: vertical placement by layoutRect (qlabel.cpp:1624-1637).
    for name, align in [("top", A.AlignLeft | A.AlignTop), ("bottom", A.AlignLeft | A.AlignBottom)]:
        l = QLabel("<b>Hg</b>")
        l.setTextFormat(Qt.RichText)
        l.setFont(f)
        l.setAlignment(align)
        l.resize(200, 60)
        out(f"item5_rich_label_ink_{name}", ink(l))
    # Text taller than the label: a rich label never starts above its rect (qMax(..., 0) in
    # layoutRect, qlabel.cpp:1632-1635); plain text is offset by (h - height) / 2 unclamped
    # (qpainter.cpp:7421-7424), then clipped.
    for name, fmt, align in [
        ("rich_vcenter", Qt.RichText, A.AlignLeft | A.AlignVCenter),
        ("rich_bottom", Qt.RichText, A.AlignLeft | A.AlignBottom),
        ("rich_top", Qt.RichText, A.AlignLeft | A.AlignTop),
        ("plain_vcenter", Qt.PlainText, A.AlignLeft | A.AlignVCenter),
        ("plain_top", Qt.PlainText, A.AlignLeft | A.AlignTop),
    ]:
        l = QLabel("<b>Hg</b>" if fmt == Qt.RichText else "Hg")
        l.setTextFormat(fmt)
        l.setFont(f)
        l.setAlignment(align)
        l.resize(200, 8)
        out(f"item5_short_label_ink_{name}", ink(l))


def main():
    app = QApplication.instance() or QApplication(sys.argv)
    print(f"qt={__import__('PySide6').__version__} platform={app.platformName()}")
    item1()
    item2()
    item3()
    item4()
    item5()


if __name__ == "__main__":
    main()
