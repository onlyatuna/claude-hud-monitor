"""Minimal frameless translucent QWidget (no qtrs). Corner press -> startSystemResize."""
import sys
from PySide6.QtCore import Qt, QRectF
from PySide6.QtGui import QPainter, QColor, QPainterPath
from PySide6.QtWidgets import QApplication, QWidget

M = 20


class W(QWidget):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("SLTEST")
        self.setWindowFlags(Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint)
        self.setAttribute(Qt.WA_TranslucentBackground)
        self.setMinimumSize(100, 100)
        self.setGeometry(200, 150, 500, 400)

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.setCompositionMode(QPainter.CompositionMode_Source)
        path = QPainterPath()
        path.addRoundedRect(QRectF(self.rect()), 24, 24)
        p.fillPath(path, QColor(255, 0, 255))

    def mousePressEvent(self, e):
        x, y = e.position().x(), e.position().y()
        l, t, r, b = x < M, y < M, self.width() - x < M, self.height() - y < M
        ed = None
        if r and b: ed = Qt.RightEdge | Qt.BottomEdge
        elif l and t: ed = Qt.LeftEdge | Qt.TopEdge
        elif r and t: ed = Qt.RightEdge | Qt.TopEdge
        elif l and b: ed = Qt.LeftEdge | Qt.BottomEdge
        if ed is not None:
            self.windowHandle().startSystemResize(ed)


a = QApplication(sys.argv)
w = W(); w.show()
a.exec()
