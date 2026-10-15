"""G9.4.a stage 1: Qt default layout margin/spacing by owner kind (offscreen)."""
import sys
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication, QWidget, QHBoxLayout, QGridLayout, QVBoxLayout, QStyle

app = QApplication(sys.argv)
print("dpr", app.primaryScreen().devicePixelRatio(), "logicalDpi", app.primaryScreen().logicalDotsPerInchX())
def m(l):
    c = l.contentsMargins(); return (c.left(), c.top(), c.right(), c.bottom())
def owners():
    top = QWidget(); par = QWidget(); child = QWidget(par)
    popup_par = QWidget(); popup = QWidget(popup_par, Qt.Popup)
    tool = QWidget(popup_par, Qt.Tool); dlg = QWidget(popup_par, Qt.Window)
    return {"top": top, "child": child, "popup_with_parent": popup, "tool_with_parent": tool,
            "window_with_parent": dlg}, (par, popup_par)
for kind in (QHBoxLayout, QVBoxLayout, QGridLayout):
    ow, keep = owners()
    for name, w in ow.items():
        l = kind(w)
        s = (l.horizontalSpacing(), l.verticalSpacing()) if kind is QGridLayout else l.spacing()
        print(kind.__name__, name, "isWindow", w.isWindow(), "margins", m(l), "spacing", s)
# nested layouts and parentless
for kind in (QHBoxLayout, QGridLayout):
    l = kind()
    s = (l.horizontalSpacing(), l.verticalSpacing()) if kind is QGridLayout else l.spacing()
    print(kind.__name__, "parentless", m(l), s)

# Owned grid size hint: two fixed 50x20 items side by side / stacked, top-level owner.
def fixed():
    w = QWidget(); w.setFixedSize(50, 20); return w
for horiz in (True, False):
    top = QWidget(); g = QGridLayout(top)
    a, b = fixed(), fixed()
    if horiz: g.addWidget(a, 0, 0); g.addWidget(b, 0, 1)
    else: g.addWidget(a, 0, 0); g.addWidget(b, 1, 0)
    h = g.sizeHint(); print("owned grid", "horizontal" if horiz else "vertical", "hint", (h.width(), h.height()))
