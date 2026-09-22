"""
Table-style HUD body: providers are columns, row labels are shared on the left.

Each column shows the 5-hour window (reset time, time left) and the weekly
window (percentage, reset time, time left) around one dial:
  inner pie  = 5-hour usage
  outer ring = weekly usage
A dashed tick marks even pace; usage past it is hatched. Pace never changes
colour, so it cannot be confused with the colour scheme.
"""
import math
from typing import Optional

from PySide6.QtCore import Qt, QRectF, QPointF
from PySide6.QtGui import (QPainter, QPen, QColor, QFont, QPainterPath, QFontMetricsF, QPixmap, QBrush,
                           QPainterPathStroker)
from PySide6.QtWidgets import QWidget, QLabel, QGridLayout, QHBoxLayout, QVBoxLayout, QSizePolicy, QFrame

from core.pace import (FIVE_HOURS, ONE_WEEK, elapsed_fraction, format_countdown_dhm, format_countdown_hm,
                       format_reset_time, pace_mark, runout_text, scale_level, window_caption, window_seconds)
from core.providers.base import UsageMetrics
from ui.styles import get_table_stylesheet

PROVIDER_NAMES = {"claude": "Claude Code", "codex": "Codex", "agy": "Antigravity"}
PROVIDER_ORDER = ["claude", "codex", "agy"]

LABELS = {"five_h": "5 小時", "week": "1 週", "reset": "重設", "left": "剩餘",
          "legend_inner": "內圈 5 小時", "legend_outer": "外環 1 週",
          "legend_pace": "平均進度", "legend_over": "超出平均"}


def _hatch_brush(color: QColor) -> QBrush:
    tile = QPixmap(6, 6)
    tile.fill(Qt.GlobalColor.transparent)
    p = QPainter(tile)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    p.setPen(QPen(color, 1.6))
    p.drawLine(QPointF(-1, 7), QPointF(7, -1))
    p.drawLine(QPointF(-1, 1), QPointF(1, -1))
    p.drawLine(QPointF(5, 7), QPointF(7, 5))
    p.end()
    return QBrush(tile)


def _hidpi_pixmap(size: int) -> QPixmap:
    pm = QPixmap(size * 2, size * 2)
    pm.fill(Qt.GlobalColor.transparent)
    pm.setDevicePixelRatio(2)
    return pm


def provider_icon(provider_id: str, theme: dict, size: int = 18) -> QPixmap:
    """Monochrome line glyph per provider in the text colour (no brand artwork)."""
    pm = _hidpi_pixmap(size)
    p = QPainter(pm)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    p.setPen(QPen(QColor(theme["text"]), 1.5, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap,
                  Qt.PenJoinStyle.RoundJoin))
    p.setBrush(Qt.BrushStyle.NoBrush)
    s, c = float(size), size / 2
    if provider_id == "claude":      # radiating burst
        for i in range(8):
            a = math.radians(i * 45 + 22.5)
            r1, r2 = s * 0.14, s * (0.44 if i % 2 else 0.36)
            p.drawLine(QPointF(c + r1 * math.cos(a), c + r1 * math.sin(a)),
                       QPointF(c + r2 * math.cos(a), c + r2 * math.sin(a)))
    elif provider_id == "codex":     # terminal prompt
        p.drawRoundedRect(QRectF(s * 0.1, s * 0.18, s * 0.8, s * 0.64), s * 0.14, s * 0.14)
        p.drawPolyline([QPointF(s * 0.28, s * 0.38), QPointF(s * 0.42, s * 0.5), QPointF(s * 0.28, s * 0.62)])
        p.drawLine(QPointF(s * 0.5, s * 0.64), QPointF(s * 0.7, s * 0.64))
    else:                            # arch lifting off
        path = QPainterPath(QPointF(s * 0.14, s * 0.86))
        path.cubicTo(QPointF(s * 0.28, s * 0.1), QPointF(s * 0.72, s * 0.1), QPointF(s * 0.86, s * 0.86))
        p.drawPath(path)
        p.drawLine(QPointF(s * 0.34, s * 0.62), QPointF(s * 0.66, s * 0.62))
    p.end()
    return pm


def legend_glyph(kind: str, color: str, theme: dict, size: int = 12) -> QPixmap:
    """Tiny swatches matching what the dial draws."""
    pm = _hidpi_pixmap(size)
    p = QPainter(pm)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    r = QRectF(1.5, 1.5, size - 3, size - 3)
    if kind == "pie":
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(*theme["disc"]).darker(110))
        p.drawEllipse(r)
        p.setBrush(QColor(color))
        p.drawPie(r, 90 * 16, -250 * 16)
    elif kind == "ring":
        inset = r.adjusted(1, 1, -1, -1)
        p.setPen(QPen(QColor(*theme["track"]), 2.2))
        p.drawEllipse(inset)
        p.setPen(QPen(QColor(color), 2.2))
        p.drawArc(inset, 90 * 16, -250 * 16)
    elif kind == "tick":
        pen = QPen(QColor(theme["text"]), 1.4, Qt.PenStyle.CustomDashLine)
        pen.setDashPattern([1.5, 1.3])
        p.setPen(pen)
        p.drawLine(QPointF(size / 2, 1), QPointF(size / 2, size - 1))
    elif kind == "hatch":
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(color))
        p.drawRoundedRect(r, 2, 2)
        p.fillRect(r, _hatch_brush(QColor(*theme["hatch"])))
    p.end()
    return pm


def _glyph_row(kind: str, color: str, text: str, theme: dict, obj: str, size: int = 12) -> QWidget:
    row = QWidget()
    h = QHBoxLayout(row)
    h.setContentsMargins(0, 0, 0, 0)
    h.setSpacing(5)
    icon = QLabel()
    icon.setPixmap(legend_glyph(kind, color, theme, size))
    h.addWidget(icon)
    lbl = QLabel(text)
    lbl.setObjectName(obj)
    h.addWidget(lbl)
    h.addStretch()
    return row


class UsageDial(QWidget):
    """Inner pie = 5h window, outer ring = weekly window."""

    def __init__(self, theme: dict, parent=None):
        super().__init__(parent)
        self.t = theme
        self.inner = (None, None, None)   # (percent, pace mark, colour)
        self.outer = (None, None, None)
        self.inner_text = "--"
        self.caption = ""
        self.muted = False
        self.setMinimumSize(84, 84)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_values(self, inner, inner_text, outer, caption="", muted=False):
        self.inner, self.inner_text, self.outer = inner, inner_text, outer
        self.caption, self.muted = caption, muted
        self.update()

    @staticmethod
    def _deg(percent: float) -> float:
        return -360 * min(100.0, max(0.0, percent)) / 100

    def _pace_tick(self, p, center, r_from, r_to, mark):
        if mark is None or self.muted:
            return
        a = math.radians(90 - 360 * mark / 100)
        pen = QPen(QColor(self.t["text"]), 1.6, Qt.PenStyle.CustomDashLine)
        pen.setDashPattern([1.6, 1.4])
        p.setPen(pen)
        p.drawLine(QPointF(center.x() + r_from * math.cos(a), center.y() - r_from * math.sin(a)),
                   QPointF(center.x() + r_to * math.cos(a), center.y() - r_to * math.sin(a)))

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        side = min(self.width(), self.height()) - 2
        outer = QRectF((self.width() - side) / 2, (self.height() - side) / 2, side, side)
        center = outer.center()
        ring = max(5.0, side * 0.07)
        hatch = _hatch_brush(QColor(*self.t["hatch"]))

        # Outer ring: weekly
        o_pct, o_mark, o_color = self.outer
        ring_rect = outer.adjusted(ring / 2 + 1, ring / 2 + 1, -ring / 2 - 1, -ring / 2 - 1)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.setPen(QPen(QColor(*self.t["track"]), ring))
        p.drawEllipse(ring_rect)
        if o_pct is not None and not self.muted:
            pen = QPen(QColor(o_color), ring)
            pen.setCapStyle(Qt.PenCapStyle.FlatCap)
            p.setPen(pen)
            p.drawArc(ring_rect, 90 * 16, int(self._deg(o_pct) * 16))
            if o_mark is not None and o_pct > o_mark:
                arc = QPainterPath()
                arc.arcMoveTo(ring_rect, 90 + self._deg(o_mark))
                arc.arcTo(ring_rect, 90 + self._deg(o_mark), self._deg(o_pct) - self._deg(o_mark))
                stroker = QPainterPathStroker()
                stroker.setWidth(ring)
                stroker.setCapStyle(Qt.PenCapStyle.FlatCap)
                p.fillPath(stroker.createStroke(arc), hatch)
        self._pace_tick(p, center, side / 2 - ring - 2.5, side / 2 + 0.5, o_mark)

        # Inner pie: 5h
        i_pct, i_mark, i_color = self.inner
        gap = ring + side * 0.06
        inner = outer.adjusted(gap, gap, -gap, -gap)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(*self.t["disc"]))
        p.drawEllipse(inner)
        if i_pct is not None and not self.muted:
            p.setBrush(QColor(i_color))
            if i_pct >= 100:
                p.drawEllipse(inner)
            elif i_pct > 0:
                p.drawPie(inner, 90 * 16, int(self._deg(i_pct) * 16))
            if i_mark is not None and i_pct > i_mark:
                wedge = QPainterPath(center)
                wedge.arcTo(inner, 90 + self._deg(i_mark), self._deg(i_pct) - self._deg(i_mark))
                wedge.closeSubpath()
                p.fillPath(wedge, hatch)
        r_in = inner.width() / 2
        self._pace_tick(p, center, r_in * 0.6, r_in, i_mark)

        # Centre text with a halo so it reads over the pie and the background alike
        has_pct = self.inner_text.endswith("%")
        num = self.inner_text.replace("%", "").strip()
        f = QFont(self.font())
        f.setWeight(QFont.Weight.Bold)
        f.setPixelSize(max(8, int(inner.height() * (0.30 if has_pct else 0.22))))
        suf = QFont(f)
        suf.setPixelSize(max(6, int(f.pixelSize() * 0.5)))
        cy = center.y() - (inner.height() * 0.05 if self.caption else 0)
        fm = QFontMetricsF(f)
        num_w = fm.horizontalAdvance(num)
        suf_w = QFontMetricsF(suf).horizontalAdvance("%") + 1 if has_pct else 0
        x = center.x() - (num_w + suf_w) / 2
        base = cy + fm.capHeight() / 2
        path = QPainterPath()
        path.addText(QPointF(x, base), f, num)
        if has_pct:
            path.addText(QPointF(x + num_w + 1, base), suf, "%")
        if not self.muted:
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.setPen(QPen(QColor(*self.t["halo"]), 3, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap,
                          Qt.PenJoinStyle.RoundJoin))
            p.drawPath(path)
        p.fillPath(path, QColor(self.t["text3"] if self.muted else self.t["text"]))

        if self.caption:
            cf = QFont(self.font())
            cf.setBold(True)
            cf.setPixelSize(max(9, int(inner.height() * 0.13)))
            cap_path = QPainterPath()
            cw = QFontMetricsF(cf).horizontalAdvance(self.caption)
            cap_path.addText(QPointF(center.x() - cw / 2, center.y() + inner.height() * 0.32), cf, self.caption)
            p.setPen(QPen(QColor(*self.t["halo"]), 2.5))
            p.drawPath(cap_path)
            p.fillPath(cap_path, QColor(self.t["text"]))
        p.end()


def _cell(text: str, obj: str) -> QLabel:
    lbl = QLabel(text)
    lbl.setObjectName(obj)
    lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
    return lbl


def _set_state(widget: QWidget, state: str):
    if widget.property("state") != state:
        widget.setProperty("state", state)
        widget.style().unpolish(widget)
        widget.style().polish(widget)


class ProviderColumn:
    """All cells for one provider; UsageTable places them into its grid."""

    def __init__(self, provider_id: str, theme: dict, scheme: str):
        self.provider_id = provider_id
        self.t = theme
        self.scheme = scheme
        self.current_metrics = UsageMetrics(provider_id=provider_id)

        self.header = QWidget()
        hv = QVBoxLayout(self.header)
        hv.setContentsMargins(0, 0, 0, 0)
        hv.setSpacing(0)
        top = QHBoxLayout()
        top.setSpacing(5)
        top.addStretch()
        self.icon = QLabel()
        self.icon.setPixmap(provider_icon(provider_id, theme))
        top.addWidget(self.icon)
        self.name = QLabel(PROVIDER_NAMES.get(provider_id, provider_id))
        self.name.setObjectName("HeaderName")
        top.addWidget(self.name)
        top.addStretch()
        hv.addLayout(top)
        self.badge = _cell(" ", "HeaderBadge")
        hv.addWidget(self.badge)

        self.m1_reset = _cell("--:--", "Cell")
        self.m1_countdown = _cell("--:--", "Cell")
        self.dial = UsageDial(theme)
        self.m2_val = _cell("--", "Pill")
        self.m2_reset = _cell("--:--", "Cell")
        self.m2_countdown = _cell("--:--:--", "Cell")
        self.value_cells = [self.m1_reset, self.m1_countdown, self.m2_val, self.m2_reset, self.m2_countdown]
        for w in self.value_cells:
            f = w.font()
            f.setFeature(QFont.Tag("tnum"), 1)  # tabular digits keep countdowns from jittering
            w.setFont(f)

    def _colors(self, percent: Optional[float], which: str):
        """(fill colour, text colour) for one window under the active scheme."""
        if self.scheme == "duo":
            return self.t["duo"][which], self.t["duo_text"][which]
        level = scale_level(percent)
        if level is None:
            return self.t["neutral"], self.t["text2"]
        return self.t["scale"][level], self.t["scale_text"][level]

    def _is_offline(self) -> bool:
        data = self.current_metrics
        return bool(data.error) and not data.stale

    def update_metrics(self, data: UsageMetrics):
        self.current_metrics = data
        for w in self.value_cells + [self.header]:
            w.setToolTip(data.error or "")

        muted = "muted" if self._is_offline() else ""
        for w in self.value_cells + [self.name]:
            _set_state(w, muted)
        self.icon.setEnabled(not muted)

        if muted:
            self.badge.setText("OFFLINE")
            self.dial.set_values((None, None, None), "ERR", (None, None, None), muted=True)
            self.dial.setToolTip(data.error or "")
            self.m2_val.setText("--")
            self.m2_val.setStyleSheet("")
            for w, text in ((self.m1_reset, "--:--"), (self.m1_countdown, "--:--"),
                            (self.m2_reset, "--:--"), (self.m2_countdown, "--:--:--")):
                w.setText(text)
            return

        self.badge.setText("⏱ STALE" if data.stale else (data.badge1_text or data.badge2_text or " "))
        if data.stale:
            stamp = data.last_success.astimezone().strftime("%m/%d %H:%M:%S") if data.last_success else "--"
            self.header.setToolTip("\n".join(s for s in (f"舊資料 {stamp}", data.error) if s))
        self.update_countdown()

    def update_countdown(self):
        """Refresh everything that depends on the clock: countdowns, pace marks, run-out estimate."""
        if self._is_offline():
            return
        data = self.current_metrics
        self.m1_reset.setText(format_reset_time(data.metric1_reset, with_day=False))
        self.m1_countdown.setText(format_countdown_hm(data.metric1_reset))
        self.m2_reset.setText(format_reset_time(data.metric2_reset, with_day=True))
        self.m2_countdown.setText(format_countdown_dhm(data.metric2_reset))

        w1 = window_seconds(data.metric1_title, FIVE_HOURS)
        w2 = window_seconds(data.metric2_title, ONE_WEEK)
        e1 = elapsed_fraction(data.metric1_reset, w1)
        e2 = elapsed_fraction(data.metric2_reset, w2)
        mark1, mark2 = pace_mark(e1), pace_mark(e2)
        fill1, _ = self._colors(data.metric1_val, "inner")
        fill2, text2 = self._colors(data.metric2_val, "outer")
        self.dial.set_values((data.metric1_val, mark1, fill1), data.metric1_text,
                             (data.metric2_val, mark2, fill2), window_caption(data.metric1_title, "5H"))

        pct = data.metric2_text.replace("%", " %")
        cap2 = window_caption(data.metric2_title, "7D")
        text = f"{pct} · {cap2}" if cap2 else pct
        if data.metric2_val is not None and mark2 is not None and data.metric2_val - mark2 > 0.5:
            text += f"  ▲{data.metric2_val - mark2:.0f}"
        self.m2_val.setText(text)
        self.m2_val.setStyleSheet(f"color: {text2};")

        tip1 = runout_text(data.metric1_val, e1, w1)
        tip2 = runout_text(data.metric2_val, e2, w2)
        stale_tip = data.error if data.stale and data.error else ""
        self.m2_val.setToolTip("\n".join(s for s in (tip2, stale_tip) if s))
        self.dial.setToolTip("\n".join(s for s in (tip1 and f"5 小時：{tip1}", tip2 and f"1 週：{tip2}",
                                                   stale_tip) if s))


class UsageTable(QWidget):
    def __init__(self, theme: dict, scheme: str = "scale", provider_ids=PROVIDER_ORDER, parent=None):
        super().__init__(parent)
        self.setStyleSheet(get_table_stylesheet(theme))
        self.columns = {pid: ProviderColumn(pid, theme, scheme) for pid in provider_ids}

        grid = QGridLayout(self)
        grid.setContentsMargins(0, 0, 0, 0)
        grid.setHorizontalSpacing(10)
        grid.setVerticalSpacing(4)

        def sub_label(text, row):
            lbl = QLabel(text)
            lbl.setObjectName("RowLabel")
            grid.addWidget(lbl, row, 0, Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft)

        def separator(row):
            line = QFrame()
            line.setObjectName("Separator")
            grid.addWidget(line, row, 0, 1, len(self.columns) + 1)

        if scheme == "duo":
            inner_c, outer_c, hatch_c = theme["duo"]["inner"], theme["duo"]["outer"], theme["duo"]["inner"]
        else:
            inner_c = outer_c = theme["neutral"]
            hatch_c = theme["scale"]["yellow"]

        # 5-hour section: title, reset, time left, then the dial with its legend beside it
        separator(1)
        grid.addWidget(_glyph_row("pie", inner_c, LABELS["five_h"], theme, "SectionTitle", 13), 2, 0)
        sub_label(LABELS["reset"], 3)
        sub_label(LABELS["left"], 4)
        legend = QWidget()
        lv = QVBoxLayout(legend)
        lv.setContentsMargins(0, 0, 0, 0)
        lv.setSpacing(3)
        for kind, color, key in (("pie", inner_c, "legend_inner"), ("ring", outer_c, "legend_outer"),
                                 ("tick", "", "legend_pace"), ("hatch", hatch_c, "legend_over")):
            lv.addWidget(_glyph_row(kind, color, LABELS[key], theme, "Legend", 11))
        grid.addWidget(legend, 5, 0, Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft)

        # Weekly section: the title row carries the weekly percentage
        separator(6)
        grid.addWidget(_glyph_row("ring", outer_c, LABELS["week"], theme, "SectionTitle", 13), 7, 0)
        sub_label(LABELS["reset"], 8)
        sub_label(LABELS["left"], 9)

        for col, column in enumerate(self.columns.values(), start=1):
            grid.addWidget(column.header, 0, col)
            grid.addWidget(column.m1_reset, 3, col)
            grid.addWidget(column.m1_countdown, 4, col)
            grid.addWidget(column.dial, 5, col)
            grid.addWidget(column.m2_val, 7, col, Qt.AlignmentFlag.AlignHCenter)
            grid.addWidget(column.m2_reset, 8, col)
            grid.addWidget(column.m2_countdown, 9, col)
            grid.setColumnStretch(col, 1)
        grid.setRowStretch(5, 1)
        grid.setRowMinimumHeight(2, 18)

    def update_metrics(self, data: UsageMetrics):
        if data.provider_id in self.columns:
            self.columns[data.provider_id].update_metrics(data)

    def update_countdowns(self):
        for column in self.columns.values():
            column.update_countdown()
