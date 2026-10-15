"""ProviderCard under dynamic relayout (G9.7.b, sub-scope 3).

The real `ProviderCardWidget` of `python/ui/provider_card.py` with the real cards style sheet
(`get_cards_stylesheet(vibrant=True, dark=True)`, the one the Rust oracle tests install), fed the
`UsageMetrics` the HUD feeds it. After each `update_metrics` the rectangle of every child is printed,
relative to the card (hidden children print `-`). The card is 256 wide (the vertical HUD's content
width) and 140 high, in an unshown top-level widget: its layout is invalidated and activated after
each update, standing for the `LayoutRequest` that `setVisible`/`setText` post to a shown window.
The owner has an explicit minimum size of (100, 0), as the HUD windows have, so a narrow step is
squeezed instead of growing the top-level widget.

Steps (the Rust test in `rust/src/ui/provider_card_relayout_tests.rs` runs the same ones):
  initial       just constructed
  both_badges   badge1 "Code: 100%", badge2 "Chat: 0%"
  one_badge     badge1 "Plan: Free", badge2 empty
  error         error set, not stale (OFFLINE badge, m1 "ERR")
  stale         error set and stale, last_success set (STALE badge)
  long_title    provider_name "OPENAI CODEX EXTRA LONG NAME", both badges, metric titles changed
  narrow        the same data at 180 wide
  empty         back to a default `UsageMetrics` (badge "--", badge2 hidden), 256 wide
`metric*_subtext` (Python only) is left empty on both sides.

Run (Windows; PySide6 6.11.2): DPR 1.25 -> QT_SCREEN_SCALE_FACTORS=1.25; DPR 1.0 -> QT_ENABLE_HIGHDPI_SCALING=0
  python provider_card_relayout.py
"""

import sys
from datetime import datetime, timezone
from pathlib import Path

PY_ROOT = str(Path(__file__).resolve().parents[4] / "python")
sys.path.insert(0, PY_ROOT)

from PySide6.QtWidgets import QApplication, QVBoxLayout, QWidget

from core.providers.base import UsageMetrics
from ui.provider_card import ProviderCardWidget
from ui.styles import get_cards_stylesheet

NAMES = (
    "dot", "title", "badge", "badge2",
    "m1_label", "m1_val", "m1_bar", "m1_sub",
    "m2_label", "m2_val", "m2_bar", "m2_sub",
)


def rect(w):
    if w.isHidden():
        return "-"
    g = w.geometry()
    return f"{g.x()},{g.y()},{g.width()},{g.height()}"


def main():
    app = QApplication(sys.argv)
    app.setStyleSheet(get_cards_stylesheet(True, dark=True))

    owner = QWidget()
    card = ProviderCardWidget("codex", owner)
    outer = QVBoxLayout(owner)
    outer.setContentsMargins(0, 0, 0, 0)
    outer.addWidget(card)
    owner.setMinimumSize(100, 0)

    def show(step, width):
        owner.resize(width, 140)
        outer.invalidate()
        outer.activate()
        card.layout().invalidate()
        card.layout().activate()
        hint = card.sizeHint()
        print(f"{step} hint={hint.width()}x{hint.height()} "
              + " ".join(f"{n}={rect(getattr(card, n))}" for n in NAMES))

    print(f"dpr={app.devicePixelRatio()}")
    show("initial", 256)

    def feed(step, width=256, **kw):
        base = dict(provider_id="codex", metric1_val=68.0, metric1_text="68%",
                    metric2_val=100.0, metric2_text="100%")
        base.update(kw)
        card.update_metrics(UsageMetrics(**base))
        show(step, width)

    feed("both_badges", badge1_text="Code: 100%", badge2_text="Chat: 0%")
    feed("one_badge", badge1_text="Plan: Free")
    feed("error", error="Network error")
    feed("stale", error="Network error", stale=True,
         last_success=datetime(2026, 1, 2, 3, 4, 5, tzinfo=timezone.utc))
    long = dict(provider_name="OPENAI CODEX EXTRA LONG NAME", badge1_text="Code: 100%",
                badge2_text="Chat: 0%", metric1_title="SESSION 5H LONG TITLE",
                metric2_title="WEEKLY 7D LONG TITLE")
    feed("long_title", **long)
    feed("narrow", width=180, **long)
    card.update_metrics(UsageMetrics(provider_id="codex"))
    show("empty", 256)


main()
