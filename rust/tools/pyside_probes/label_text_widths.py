"""`QLabel::sizeHint` widths of three HUD header/metric texts under the HUD's QLabel style sheet.

The Rust side is `rust/tests/test_label_text_metric_rounding.rs`: `MetricTitle` "WEEKLY 7D",
`HeaderTitle` "AI AGENT HUD (3-IN-1)" and `HeaderStatus` "Updated 12:34:56" (Consolas 9.5 px), with the
sheet's `QLabel` rule `font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif`.
The sheet below is the one that test uses: a reduced copy of the HUD's QLabel rules (`python/ui/styles.py`).

Run (Windows; PySide6 6.12.0): DPR 1.25 -> QT_SCREEN_SCALE_FACTORS=1.25; DPR 1.0 -> QT_ENABLE_HIGHDPI_SCALING=0
  python label_text_widths.py
Prints `<Qt version> <dpr> [MetricTitle, HeaderTitle, HeaderStatus]`.
"""
import sys

from PySide6.QtCore import qVersion
from PySide6.QtWidgets import QApplication, QLabel

SHEET = """
QLabel { color: #e2e8f0; font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif; }
QLabel#HeaderTitle { font-size: 10.5px; font-weight: 800; letter-spacing: 1.0px; color: #94a3b8; }
QLabel#MetricTitle { font-size: 10px; font-weight: 700; color: #94a3b8; letter-spacing: 0.6px; }
QLabel#HeaderStatus { font-size: 9.5px; color: #64748b; font-family: 'Consolas', monospace; }
"""

app = QApplication(sys.argv)
app.setStyleSheet(SHEET)
widths = []
for name, text in (("MetricTitle", "WEEKLY 7D"), ("HeaderTitle", "AI AGENT HUD (3-IN-1)"),
                   ("HeaderStatus", "Updated 12:34:56")):
    label = QLabel(text)
    label.setObjectName(name)
    widths.append(label.sizeHint().width())
print(qVersion(), app.devicePixelRatio(), widths)
