"""Label size hints with `letter-spacing: 0.4px` (the HUD's provider card title, `provider_card.py:38`).

`QFont::setLetterSpacing` stores `QFixed::fromReal(0.4)` = 25/64 px (truncated to 26.6 fixed point,
`qfont.cpp`, `qfixed_p.h`), added to every glyph advance (`QTextEngine::shapeText`), so the width of a
long text differs by a pixel from one that adds 0.4 exactly. Prints `text width height` of
`QLabel.sizeHint()` for the card-title style at the style sheet of
`crates/ferrokit-widgets/tests/test_label_letter_spacing_hint.rs` in the ferrokit repository.

Run (Windows; PySide6 6.11.2): DPR 1.25 -> QT_SCREEN_SCALE_FACTORS=1.25; DPR 1.0 -> QT_ENABLE_HIGHDPI_SCALING=0
  python label_letter_spacing.py
"""

import sys

from PySide6.QtWidgets import QApplication, QLabel

SHEET = """\
QLabel { color: #e2e8f0; font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif; }
QLabel#Title { font-size: 9.5px; font-weight: 800; letter-spacing: 0.4px; color: #a855f7; }"""
TEXTS = ("OPENAI CODEX", "OPENAI CODEX EXTRA LONG NAME", "AAAAAAAAAA", "OPENAI CODEX EXTRA")

app = QApplication(sys.argv)
app.setStyleSheet(SHEET)
print(f"dpr={app.devicePixelRatio()}")
for text in TEXTS:
    label = QLabel(text)
    label.setObjectName("Title")
    hint = label.sizeHint()
    print(f"{text!r} {hint.width()} {hint.height()}")
