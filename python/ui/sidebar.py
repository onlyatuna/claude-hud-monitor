"""Scalable quota tiles and reusable provider-card flyouts.

Compact desktop tokens: 4 px rhythm, 12 px labels, native buttons, a visible
2 px focus ring and paired AA text colours; hover is supplementary to keyboard
and click activation. Motion honours the system accessibility preference.
"""
import math
import re
import sys
from PySide6.QtCore import Qt, Signal, QSize, QRect, QRectF, QPoint, QPointF, QTimer, QEvent, QPropertyAnimation, QEasingCurve
from PySide6.QtGui import QPainter, QColor, QPen, QFont, QGuiApplication, QCursor, QPolygonF, QLinearGradient
from PySide6.QtWidgets import QWidget, QPushButton, QBoxLayout, QVBoxLayout, QFrame, QSizePolicy
from core.pace import scale_level, short_window, reset_stamp
from core.providers.base import BaseProvider
from ui.provider_card import PROVIDER_THEMES
from ui.brand import BRAND, brand_color
from ui.styles import THEMES, get_cards_stylesheet

SIDEBAR_TOKENS = {
    'dark': dict(background='#1e1e24', text='#f2f2f7', muted='#a6a6b3', hover='#30303a', pressed='#40404c', focus='#38bdf8', separator='#454550', stale='#f59e0b', alert='#dc2626', providers={k: v['dark'] for k, v in BRAND.items()}, font='Segoe UI', font_size=12, spacing=4, radius=6, focus_width=2, hover_ms=300, leave_ms=600, animation_ms=160),
    'light': dict(background='#f6f4fa', text='#1f1f24', muted='#555563', hover='#e6e4ed', pressed='#d8d6e1', focus='#0369a1', separator='#ceced8', stale='#92400e', alert='#b91c1c', providers={k: v['light'] for k, v in BRAND.items()}, font='Segoe UI', font_size=12, spacing=4, radius=6, focus_width=2, hover_ms=300, leave_ms=600, animation_ms=160),
}
BASE_BUTTON_W, BASE_BUTTON_H = 64, 64
BASE_PANEL_W = 220
_NAMES = {'claude':'Claude', 'agy':'AGY', 'codex':'Codex'}
DEFAULT_ORDER = ('claude', 'agy', 'codex')
HOLD_MS = 350
RAMP_STOPS = ((0, 'green'), (50, 'yellow'), (75, 'orange'), (90, 'red'))   # same bands as the status scale
ALERT_PCT = 90          # a row at or above this used % marks the tile
FLASH_MS = 1800         # how long a tile glows after one of its windows reset
FLASH_STEP_MS = 60


def _row_label(title):
    """Short row tag: 5H / 7D for single-group providers, a two-letter group prefix for agy's groups."""
    upper = (title or '').upper()
    for prefix, tag in (('GEMINI ', 'Ge '), ('C/G ', 'CG '), ('ARGON ', 'Ar ')):
        if upper.startswith(prefix):
            return tag + upper[len(prefix):]
    return short_window(title or '')


class _Indicators(QWidget):
    """Paint status above the tile without intercepting its button's input."""
    def __init__(self, button):
        super().__init__(button)
        self.button = button
        self.setAttribute(Qt.WA_TransparentForMouseEvents)
        self.setAttribute(Qt.WA_NoSystemBackground)

    def paintEvent(self, event):
        button = self.button
        t, scale = button.owner.tokens, button.owner.scale()
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        right = self.width()
        if button.alert:
            # warning triangle with "!" -- readable without relying on colour
            size = 11*scale
            x, y = right-size-2*scale, 1.5*scale
            triangle = QPolygonF([QPointF(x+size/2, y), QPointF(x+size, y+size*0.92), QPointF(x, y+size*0.92)])
            p.setPen(Qt.NoPen)
            p.setBrush(QColor(t['alert']))
            p.drawPolygon(triangle)
            font = QFont(t['font'])
            font.setBold(True)
            font.setPixelSize(max(6, round(8*scale)))
            p.setFont(font)
            p.setPen(QColor('#ffffff'))
            p.drawText(QRectF(x, y+size*0.28, size, size*0.64), Qt.AlignCenter, '!')
            right -= size + 4*scale
        if button.stale:
            p.setPen(Qt.NoPen)
            p.setBrush(QColor(t['stale']))
            p.drawEllipse(QRectF(right-10*scale,0*scale,6*scale,6*scale))
        if button.kind == 'agy':
            count = len(button.dot_colors)
            for i, color in enumerate(button.dot_colors):
                p.setPen(QPen(QColor(t['background']), scale))
                p.setBrush(QColor(t['muted'] if button.offline else color))
                p.drawEllipse(QRectF((60-8*count+8*i)*scale,10*scale,6*scale,6*scale))


class _Button(QPushButton):
    def __init__(self, kind, owner):
        super().__init__(owner)
        self.kind, self.owner = kind, owner
        self.stale, self.offline = False, False
        self.alert = False
        self.flash = 0.0
        self._press_pos, self._dragged = None, False
        self.dot_colors = [THEMES['dark']['neutral']] * 3
        self._reordering = False
        self._hold = QTimer(self)
        self._hold.setSingleShot(True)
        self._hold.timeout.connect(self._begin_reorder)
        self.rows = [('5H', None, owner._color(None)), ('7D', None, owner._color(None, 'outer'))]
        self.display = ['--', '--']
        self.show_labels = owner.scale() >= 0.8
        self.indicators = _Indicators(self)
        self.setFocusPolicy(Qt.StrongFocus)
        self.setMouseTracking(True)
        self.setCursor(Qt.PointingHandCursor)
        self.setAccessibleName(_NAMES.get(kind, 'Toggle sidebar'))
        self.setToolTip(self.accessibleName())

    def event(self, event):
        # The flyout already shows the details; a native tooltip on top of it only flickers.
        if event.type() == QEvent.ToolTip and self.owner._active is not None:
            return True
        return super().event(event)

    def keyPressEvent(self, event):
        if event.key() in (Qt.Key_Return, Qt.Key_Enter):
            self.click()
        elif event.key() == Qt.Key_Escape:
            self.owner.close_flyout()
        else:
            super().keyPressEvent(event)

    def _begin_reorder(self):
        if self._press_pos is None or self._dragged or self.kind not in _NAMES or self.owner.state() != 'strip':
            return
        self._reordering = True
        self.setDown(False)
        self.setCursor(Qt.ClosedHandCursor)
        self.owner._start_reorder(self.kind)

    def mousePressEvent(self, event):
        if event.button() == Qt.LeftButton:
            self._press_pos = event.position().toPoint()
            self._dragged = False
            if self.kind in _NAMES:
                self._hold.start(HOLD_MS)
        super().mousePressEvent(event)

    def mouseMoveEvent(self, event):
        if self._reordering:
            self.owner._move_reorder(self.kind, event.globalPosition().toPoint())
            event.accept()
            return
        if self._press_pos is not None and not self._dragged:
            delta = event.position().toPoint() - self._press_pos
            if math.hypot(delta.x(), delta.y()) > 6:
                self._hold.stop()
                self._dragged = True
                self.setDown(False)
                self.owner._hover_timer.stop()
                self.owner.dragRequested.emit()
        if not self._dragged:
            super().mouseMoveEvent(event)

    def mouseReleaseEvent(self, event):
        self._hold.stop()
        if self._reordering:
            self._reordering = False
            self._press_pos = None
            self.setDown(False)
            self.setCursor(Qt.PointingHandCursor)
            self.owner._finish_reorder()
            event.accept()
            return
        dragged = self._dragged
        self._press_pos = None
        if dragged and event.button() == Qt.LeftButton:
            self.setDown(False)
            event.accept()
        else:
            super().mouseReleaseEvent(event)

    def enterEvent(self, event):
        self.update()
        if self.kind in _NAMES:
            self.owner._hover(self.kind)
        super().enterEvent(event)

    def leaveEvent(self, event):
        self.update()
        self.owner._hover_timer.stop()
        self.owner._schedule_close()
        super().leaveEvent(event)

    def paintEvent(self, event):
        t, scale = self.owner.tokens, self.owner.scale()
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(t['pressed'] if self.isDown() else t['hover'] if self.underMouse() else t['background']))
        p.drawRoundedRect(QRectF(self.rect()).adjusted(scale,scale,-scale,-scale), t['radius']*scale, t['radius']*scale)
        if self.flash > 0 and self.kind in _NAMES:
            glow = QColor(t['providers'][self.kind])
            glow.setAlphaF(min(0.45, 0.45 * self.flash))
            p.setPen(Qt.NoPen)
            p.setBrush(glow)
            p.drawRoundedRect(QRectF(self.rect()).adjusted(scale,scale,-scale,-scale), t['radius']*scale, t['radius']*scale)
        if self.kind in _NAMES:
            # identity stripe: the service colour, never a usage colour
            p.setPen(Qt.NoPen)
            p.setBrush(QColor(t['muted'] if self.offline else t['providers'][self.kind]))
            p.drawRoundedRect(QRectF(1.5*scale, 7*scale, 2.5*scale, self.height()-14*scale), 1.2*scale, 1.2*scale)
        if self.hasFocus():
            p.setBrush(Qt.NoBrush)
            color = t['providers'].get(self.kind, t['focus'])
            p.setPen(QPen(QColor(color), t['focus_width']*scale))
            p.drawRoundedRect(QRectF(self.rect()).adjusted(2*scale,2*scale,-2*scale,-2*scale), t['radius']*scale, t['radius']*scale)
        font = QFont(t['font'])
        font.setPixelSize(max(8, round(t['font_size']*scale)))
        p.setFont(font)
        if self.kind == 'toggle':
            p.setPen(QColor(t['text']))
            p.drawText(self.rect(), Qt.AlignCenter, '\u2261' if self.owner.state() == 'strip' else '\u25a5')
            return
        font.setBold(True)
        p.setFont(font)
        p.setPen(QColor(t['muted'] if self.offline else t['providers'][self.kind]))
        title_width = 30 if self.kind == 'agy' else 52
        p.drawText(QRectF(6*scale, 2*scale, title_width*scale, 16*scale), Qt.AlignLeft | Qt.AlignVCenter, _NAMES[self.kind])
        theme = THEMES['dark' if self.owner.dark else 'light']
        for index, (label, value, color) in enumerate(self.rows):
            y = (19 + index*21)*scale
            font.setPixelSize(max(8, round(10*scale)))
            font.setBold(False)
            p.setFont(font)
            p.setPen(QColor(t['muted']))
            if self.show_labels:
                p.drawText(QRectF(6*scale,y,30*scale,15*scale), Qt.AlignLeft | Qt.AlignVCenter, label)
            font.setPixelSize(max(8, round(12*scale)))
            font.setBold(True)
            p.setFont(font)
            shown = self.row_color(index)
            p.setPen(QColor(t['muted'] if self.offline else shown))
            p.drawText(QRectF(6*scale,y,52*scale,15*scale), Qt.AlignRight | Qt.AlignVCenter, self.display[index])
            track = QRectF(6*scale, y+15*scale, 52*scale, 4*scale)
            p.setPen(Qt.NoPen)
            p.setBrush(QColor(*theme['track']))
            p.drawRoundedRect(track,2*scale,2*scale)
            if value is not None and not self.offline:
                fill = QRectF(track)
                fill.setWidth(track.width()*max(0,min(100,value))/100)
                if self.owner.scheme == 'duo':
                    p.setBrush(QColor(shown))
                else:
                    # a fixed green -> amber -> red gauge revealed up to the value: the leading edge always
                    # carries the same colour as the number, and the position on the scale reads at a glance
                    gradient = QLinearGradient(track.left(), 0, track.right(), 0)
                    for pos, stop_color in self.ramp_stops():
                        gradient.setColorAt(pos/100, QColor(stop_color))
                    p.setBrush(gradient)
                p.drawRoundedRect(fill,2*scale,2*scale)

    @staticmethod
    def contrast(foreground, background):
        def luminance(color):
            rgb = QColor(color).getRgbF()[:3]
            linear = [v/12.92 if v <= 0.04045 else ((v+0.055)/1.055)**2.4 for v in rgb]
            return sum(v*w for v,w in zip(linear,(0.2126,0.7152,0.0722)))
        a,b = sorted((luminance(foreground),luminance(background)))
        return (b+0.05)/(a+0.05)

    def text_color(self, color):
        """The row colour, nudged lighter (dark theme) or darker (light theme) only as far as needed to read on the
        tile. The bar and the number share this one colour, so they can never disagree."""
        t = self.owner.tokens
        backgrounds = (t['background'], t['hover'], t['pressed'])
        if all(self.contrast(color,bg) >= 3 for bg in backgrounds):
            return color
        base = QColor(color)
        hue, sat, light, _alpha = base.getHslF()
        direction = 1 if self.owner.dark else -1
        for _ in range(40):
            light = min(1.0, max(0.0, light + 0.025*direction))
            candidate = QColor.fromHslF(max(0.0, hue), sat, light).name()
            if all(self.contrast(candidate,bg) >= 3 for bg in backgrounds):
                return candidate
        return t['text']

    def ramp_stops(self):
        """Status colours at their band starts, each made readable on the tile."""
        theme = THEMES['dark' if self.owner.dark else 'light']['scale']
        return [(pos, self.text_color(theme[name])) for pos, name in RAMP_STOPS]

    def ramp(self, value):
        """Colour of the gauge at `value` (%): a smooth blend of the readable stops."""
        stops = self.ramp_stops()
        value = max(0.0, min(100.0, float(value)))
        for (p0, c0), (p1, c1) in zip(stops, stops[1:]):
            if value <= p1:
                f = (value - p0) / (p1 - p0) if p1 > p0 else 0.0
                a, b = QColor(c0), QColor(c1)
                return QColor(round(a.red()+(b.red()-a.red())*f), round(a.green()+(b.green()-a.green())*f),
                              round(a.blue()+(b.blue()-a.blue())*f)).name()
        return QColor(stops[-1][1]).name()

    def row_color(self, index):
        """One colour per row, shared by the bar's leading edge and the number."""
        value = self.rows[index][1]
        if value is None or self.owner.scheme == 'duo':
            return self.text_color(self.rows[index][2])
        return self.ramp(value)



class _Flyout(QWidget):
    """Frameless, translucent top-level whose rounded surface is painted here (cards on top are transparent)."""
    RADIUS = 10

    def __init__(self, owner):
        super().__init__(owner, Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint)
        self.owner = owner
        self.setAttribute(Qt.WA_ShowWithoutActivating)
        self.setAttribute(Qt.WA_TranslucentBackground)

    def paintEvent(self, event):
        t = self.owner.tokens
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.setBrush(QColor(t['background']))
        p.setPen(QPen(QColor(t['separator']), 1))
        p.drawRoundedRect(QRectF(self.rect()).adjusted(0.5, 0.5, -0.5, -0.5), self.RADIUS, self.RADIUS)


class SidebarWidget(QWidget):
    stateChangeRequested = Signal(str)
    contextMenuRequested = Signal(QPoint)
    scaleStepRequested = Signal(int)
    dragRequested = Signal()
    orderChanged = Signal(list)   # provider ids in their new order (press-hold a tile, then drag)

    def __init__(self, cards: dict, parent=None):
        super().__init__(parent)
        self.cards = dict(cards)
        self._state, self._orientation, self._scale = 'strip', 'vertical', 1.0
        self._dock_edge, self.dark, self.scheme = None, True, 'scale'
        self.tokens = SIDEBAR_TOKENS['dark']
        self._metrics = {}
        self._active, self._pinned, self._hover_id = None, False, None
        self._pinned_key = None
        self._row_state = {}
        self._wheel_remainder = 0
        self._order = list(DEFAULT_ORDER)
        self._card_styles = {}
        self._hover_timer = QTimer(self)
        self._hover_timer.setSingleShot(True)
        self._hover_timer.timeout.connect(lambda: self._open_flyout(self._hover_id))
        self._leave_timer = QTimer(self)
        self._leave_timer.setSingleShot(True)
        self._leave_timer.timeout.connect(self._maybe_close)
        self._layout = QBoxLayout(QBoxLayout.TopToBottom, self)
        self.toggle_button = _Button('toggle', self)
        self.toggle_button.clicked.connect(lambda: self.stateChangeRequested.emit('panel' if self._state == 'strip' else 'strip'))
        self._layout.addWidget(self.toggle_button)
        self._strip = QWidget(self)
        self._strip_layout = QBoxLayout(QBoxLayout.TopToBottom, self._strip)
        self._strip_layout.setContentsMargins(0,0,0,0)
        self._strip_layout.setSpacing(0)
        self.buttons = {}
        for key in DEFAULT_ORDER:
            button = _Button(key, self)
            self.buttons[key] = button
            self._strip_layout.addWidget(button)
            button.clicked.connect(lambda checked=False, k=key: self._click_provider(k))
        self._layout.addWidget(self._strip)
        self._panel = QWidget(self)
        self._panel_layout = QBoxLayout(QBoxLayout.TopToBottom, self._panel)
        self._panel_layout.setContentsMargins(0,0,0,0)
        self._layout.addWidget(self._panel)
        self._panel.hide()
        self._storage = QWidget(self)
        self._storage.hide()
        for card in self.cards.values():
            card.setParent(self._storage)
            card.hide()
            card.setMinimumWidth(0)
            card.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        self._flyout = _Flyout(self)
        self._flyout_layout = QVBoxLayout(self._flyout)
        self._flyout_layout.setContentsMargins(8,8,8,8)
        self._flyout.installEventFilter(self)
        self._animation = QPropertyAnimation(self._flyout, b'pos', self)
        self._animation.setEasingCurve(QEasingCurve.OutCubic)
        self.installEventFilter(self)
        for child in self.findChildren(QWidget):
            child.installEventFilter(self)
        self.set_appearance(True)

    def state(self):
        return self._state

    def orientation(self):
        return self._orientation

    def scale(self):
        return self._scale

    def set_state(self, state):
        if state not in ('strip', 'panel'):
            raise ValueError(state)
        if state == self._state:
            return
        self.close_flyout()
        self._state = state
        self._strip.setVisible(state == 'strip')
        self._panel.setVisible(state == 'panel')
        if state == 'panel':
            for index, card in enumerate(self.cards[k] for k in self._order if k in self.cards):
                if index:
                    line = QFrame(self._panel)
                    line.setObjectName('SidebarDivider')
                    self._panel_layout.addWidget(line)
                self._panel_layout.addWidget(card)
                card.show()
        else:
            while self._panel_layout.count():
                widget = self._panel_layout.takeAt(0).widget()
                if widget in self.cards.values():
                    self._restore_card_style(widget)
                    widget.setParent(self._storage)
                    widget.hide()
                else:
                    widget.deleteLater()
        self._relayout()

    def order(self):
        return list(self._order)

    def set_order(self, order):
        order = [k for k in order if k in DEFAULT_ORDER]
        if sorted(order) != sorted(DEFAULT_ORDER):
            raise ValueError(order)
        if order == self._order:
            return
        self._order = order
        self._apply_order()

    def _apply_order(self):
        for button in self.buttons.values():
            self._strip_layout.removeWidget(button)
        for key in self._order:
            self._strip_layout.addWidget(self.buttons[key])
        self._strip_layout.activate()
        if self._state == 'panel':
            self.set_state('strip')
            self.set_state('panel')

    def _start_reorder(self, key):
        self.close_flyout()
        self._hover_timer.stop()

    def _move_reorder(self, key, global_pos):
        pos = self._strip.mapFromGlobal(global_pos)
        coordinate = pos.y() if self._orientation == 'vertical' else pos.x()
        others = [k for k in self._order if k != key]
        index = 0
        for other in others:
            geometry = self.buttons[other].geometry()
            center = (geometry.center().y() if self._orientation == 'vertical' else geometry.center().x())
            if coordinate > center:
                index += 1
        new_order = others[:index] + [key] + others[index:]
        if new_order != self._order:
            self._order = new_order
            for button in self.buttons.values():
                self._strip_layout.removeWidget(button)
            for k in self._order:
                self._strip_layout.addWidget(self.buttons[k])
            self._strip_layout.activate()

    def _finish_reorder(self):
        self.orderChanged.emit(list(self._order))

    def set_orientation(self, orientation):
        if orientation not in ('vertical', 'horizontal'):
            raise ValueError(orientation)
        if orientation == self._orientation:
            return
        self.close_flyout()
        self._orientation = orientation
        self._relayout()

    def set_dock_edge(self, edge):
        if edge not in ('left', 'right', 'top', 'bottom', None):
            raise ValueError(edge)
        if edge == self._dock_edge:
            return
        self._dock_edge = edge
        if self._active:
            self._animation.stop()
            self._flyout.move(self._flyout_position(self.buttons[self._active], self._flyout.size()))

    def set_scale(self, scale):
        scale = float(scale)
        if not math.isfinite(scale):
            raise ValueError(scale)
        scale = min(2.0, max(0.6, scale))
        if scale == self._scale:
            return
        self._scale = scale
        self._relayout()
        self.update_countdown()

    def _restore_card_style(self, card):
        for widget in [card] + card.findChildren(QWidget):
            if widget in self._card_styles:
                original, applied = self._card_styles.pop(widget)
                if widget.styleSheet() == applied:
                    widget.setStyleSheet(original)

    def _scale_card_style(self, card):
        # Local provider styles outrank container QSS. Scale their font sizes,
        # retaining the original so returning to a flyout restores readable text.
        for widget in [card] + card.findChildren(QWidget):
            current = widget.styleSheet()
            previous = self._card_styles.get(widget)
            original = previous[0] if previous and current == previous[1] else current
            applied = re.sub(r'font-size:\s*([\d.]+)px', lambda m: f'font-size: {float(m[1])*self._scale:g}px', original)
            self._card_styles[widget] = (original, applied)
            if current != applied:
                widget.setStyleSheet(applied)

    def _relayout(self):
        s, vertical = self._scale, self._orientation == 'vertical'
        direction = QBoxLayout.TopToBottom if vertical else QBoxLayout.LeftToRight
        self._layout.setDirection(direction)
        self._strip_layout.setDirection(direction)
        self._panel_layout.setDirection(direction)
        padding = round(self.tokens['spacing']*s)
        self._layout.setContentsMargins(padding,padding,padding,padding)
        self._layout.setSpacing(0)
        self._panel_layout.setSpacing(round(4*s))
        handle = round(22*s)
        self._layout.removeWidget(self.toggle_button)
        if vertical:
            self._layout.insertWidget(0, self.toggle_button)
            self.toggle_button.setMinimumSize(0, 0)
            self.toggle_button.setMaximumSize(16777215, 16777215)
            self.toggle_button.setFixedSize(round(BASE_BUTTON_W*s), handle)
        else:
            self._layout.addWidget(self.toggle_button)
            self.toggle_button.setMinimumSize(handle, handle)
            self.toggle_button.setMaximumSize(handle, 16777215)
            self.toggle_button.setSizePolicy(QSizePolicy.Fixed, QSizePolicy.Expanding)
        for button in self.buttons.values():
            button.setFixedSize(round(BASE_BUTTON_W*s),round(BASE_BUTTON_H*s))
            button.show_labels = s >= 0.8
            button.indicators.setGeometry(button.rect())
            button.indicators.raise_()
            button.indicators.update()
            button.update()
        panel_sheet = get_cards_stylesheet(dark=self.dark)
        self._panel.setStyleSheet(re.sub(r'([\d.]+)px', lambda m: f'{float(m[1])*s:g}px', panel_sheet))
        for card in self.cards.values():
            if self._state == 'panel':
                # side by side, cards keep their natural height instead of stretching to the tallest one
                self._panel_layout.setAlignment(card, Qt.AlignTop if not vertical else Qt.Alignment())
                self._scale_card_style(card)
                card.setFixedWidth(round(BASE_PANEL_W*s)-2*padding if vertical else round(BASE_PANEL_W*s))
                card.ensurePolished()
                for child in card.findChildren(QWidget):
                    child.ensurePolished()
                card.layout().activate()
            else:
                card.setMinimumWidth(0)
                card.setMaximumWidth(16777215)
        for line in self._panel.findChildren(QFrame, 'SidebarDivider'):
            line.setMinimumSize(0,0)
            line.setMaximumSize(16777215,16777215)
            if vertical:
                line.setFixedHeight(max(1,round(s)))
            else:
                line.setFixedWidth(max(1,round(s)))
        self.toggle_button.update()
        self._strip_layout.invalidate()
        self._strip_layout.activate()
        self._panel_layout.invalidate()
        self._panel_layout.activate()
        self._layout.invalidate()
        self._layout.activate()
        self.updateGeometry()

    def set_appearance(self, dark, scheme='scale'):
        if scheme not in ('scale','duo'):
            raise ValueError(scheme)
        self.dark, self.scheme = bool(dark), scheme
        self.tokens = SIDEBAR_TOKENS['dark' if dark else 'light']
        t = self.tokens
        self.setObjectName('Sidebar')
        self.setStyleSheet(get_cards_stylesheet(dark=dark)+f"QWidget#Sidebar {{background:{t['background']};border-radius:{t['radius']*self._scale}px;}} QFrame#SidebarDivider {{background:{t['separator']};border:none;}}")
        self._flyout.setStyleSheet(get_cards_stylesheet(dark=dark)+"QWidget {background-color: transparent;}")
        self._flyout.update()
        self._relayout()
        for key in self._metrics:
            self._refresh_tile(key)
        self.update_countdown()

    def _color(self, value, which='inner', force_scale=False):
        theme = THEMES['dark' if self.dark else 'light']
        if self.scheme == 'duo' and not force_scale:
            return theme['duo'][which]
        level = scale_level(value)
        return theme['scale'][level] if level else theme['neutral']

    def update_metrics(self, data):
        button = self.buttons.get(data.provider_id)
        if button is None:
            return
        previous = self._row_state.get(data.provider_id, {})
        self._metrics[data.provider_id] = data
        button.offline = bool(data.error and not data.stale)
        button.stale = bool(data.stale)
        current = self._row_state[data.provider_id] = {
            (title or '').upper(): (val, reset) for title, val, reset in self._row_values(data)}
        if not button.offline and self._window_reset(previous, current):
            self._flash(button)
        self._refresh_tile(data.provider_id)
        self._refresh_tooltip(data.provider_id)
        if self._state == 'panel':
            self._relayout()

    def _refresh_tile(self, key):
        button, data = self.buttons[key], self._metrics[key]
        values = (None, None) if button.offline else (data.metric1_val, data.metric2_val)
        labels = (_row_label(data.metric1_title), _row_label(data.metric2_title))
        button.rows = [(label, value, self._color(value, which))
                       for label,value,which in zip(labels,values,('inner','outer'))]
        button.display = ['--' if value is None else f'{value:.0f}%' for value in values]
        if button.offline:
            button.display = ['OFF', '--']
        button.dot_colors = ([self._color(row.val, force_scale=True) for row in data.extra_metrics]
                             if button.kind == 'agy' else [])
        button.alert = (not button.offline) and any(
            val is not None and val >= ALERT_PCT for _t, val, _r in self._row_values(data))
        button.indicators.update()
        button.update()

    @staticmethod
    def _row_values(data):
        rows = [(data.metric1_title, data.metric1_val, data.metric1_reset),
                (data.metric2_title, data.metric2_val, data.metric2_reset)]
        return rows + [(row.title, row.val, row.reset) for row in data.extra_metrics]

    @staticmethod
    def _window_reset(previous, current):
        """True when any row moved to a later reset time with lower usage (its window rolled over)."""
        for title, (val, reset) in current.items():
            old = previous.get(title)
            if not old or val is None or reset is None or old[0] is None or old[1] is None:
                continue
            if (reset - old[1]).total_seconds() > 600 and val < old[0]:
                return True
        return False

    def _flash(self, button):
        button.flash = 1.0
        steps = max(1, FLASH_MS // FLASH_STEP_MS)
        if not self._animations_enabled():
            QTimer.singleShot(FLASH_MS, lambda: (setattr(button, 'flash', 0.0), button.update()))
            button.update()
            return

        def tick():
            button.flash = max(0.0, button.flash - 1.0 / steps)
            button.update()
            if button.flash > 0:
                QTimer.singleShot(FLASH_STEP_MS, tick)
        button.update()
        QTimer.singleShot(FLASH_STEP_MS, tick)

    def _refresh_tooltip(self, key):
        button, data = self.buttons[key], self._metrics[key]
        lines = [_NAMES[key]]
        rows = [(data.metric1_title, data.metric1_val, data.metric1_reset),
                (data.metric2_title, data.metric2_val, data.metric2_reset)]
        rows += [(row.title, row.val, row.reset) for row in data.extra_metrics]
        for title, value, reset in rows:
            text = '--' if value is None else f'{value:.0f}%'
            label = _row_label(title) if key != 'agy' else (title or '')
            stamp = reset_stamp(reset, title)
            when = f'{stamp} · {BaseProvider.format_countdown(reset)}' if stamp else BaseProvider.format_countdown(reset)
            lines.append(f'{label} 已用 {text}，重設於 {when}')
        if button.alert:
            lines.append(f'! 接近上限（已用 ≥ {ALERT_PCT}%）')
        if data.error:
            lines.append(data.error)
        if data.stale:
            lines.append('STALE')
        button.setToolTip('\n'.join(lines))
        button.setAccessibleDescription(button.toolTip())

    def update_countdown(self):
        for key in self._metrics:
            self._refresh_tooltip(key)

    def preferred_size(self):
        s, pad = self._scale, round(self.tokens['spacing']*self._scale)*2
        vertical = self._orientation == 'vertical'
        if self._state == 'strip':
            if vertical:
                return QSize(round(BASE_BUTTON_W*s)+pad, round(22*s)+3*round(BASE_BUTTON_H*s)+pad)
            return QSize(3*round(BASE_BUTTON_W*s)+round(22*s)+pad,round(BASE_BUTTON_H*s)+pad)
        self._panel_layout.activate()
        height = self._panel_layout.sizeHint().height()
        if vertical:
            return QSize(round(BASE_PANEL_W*s),round(22*s)+height+pad)
        count = len(self.cards)
        panel_width = count*round(BASE_PANEL_W*s) + max(0,count-1)*(2*round(4*s)+max(1,round(s)))
        return QSize(round(22*s)+panel_width+pad,max(round(22*s),height)+pad)

    def _hover(self, key):
        self._leave_timer.stop()
        if self._state != 'strip':
            return
        if self._active is not None:
            if key != self._active:
                self._hover_timer.stop()
                self._open_flyout(key)
            return
        self._hover_id = key
        self._hover_timer.start(self.tokens['hover_ms'])

    def _click_provider(self, key):
        if self._pinned and self._pinned_key == key:
            self.close_flyout()
            return
        self._open_flyout(key)
        self._pinned = True
        self._pinned_key = key

    def _flyout_position(self, button, size, area=None):
        origin = button.mapToGlobal(QPoint(0,0))
        rect = QRect(origin, button.size())
        area = area if area is not None else button.screen().availableGeometry()
        if self._dock_edge == 'left':
            target = QPoint(rect.right()+1,rect.top())
        elif self._dock_edge == 'top':
            target = QPoint(rect.left(),rect.bottom()+1)
        elif self._dock_edge == 'bottom':
            target = QPoint(rect.left(),rect.top()-size.height())
        else:
            target = QPoint(rect.left()-size.width(),rect.top())
        return QPoint(max(area.left(),min(target.x(),area.right()-size.width()+1)),max(area.top(),min(target.y(),area.bottom()-size.height()+1)))

    def _open_flyout(self, key):
        if self._state != 'strip' or key not in self.cards:
            return
        if self._active == key:
            self._leave_timer.stop()
            return
        pinned, pinned_key = self._pinned, self._pinned_key
        self.close_flyout()
        self._pinned, self._pinned_key = pinned, pinned_key
        self._active = key
        card = self.cards[key]
        self._restore_card_style(card)
        card.setMinimumWidth(0)
        card.setMaximumWidth(16777215)
        self._flyout_layout.addWidget(card)
        card.show()
        button = self.buttons[key]
        area = button.screen().availableGeometry()
        self._flyout.setFixedSize(min(260,area.width()),min(self._flyout_layout.sizeHint().height(),area.height()))
        target = self._flyout_position(button,self._flyout.size(),area)
        self._flyout.move(target)
        self._flyout.show()
        if self._animations_enabled():
            self._animation.setDuration(self.tokens['animation_ms'])
            # Opacity keeps the window inside the screen throughout the animation.
            self._animation.setPropertyName(b'windowOpacity')
            self._animation.setStartValue(0.0)
            self._animation.setEndValue(1.0)
            self._animation.start()
        else:
            self._flyout.setWindowOpacity(1.0)

    @staticmethod
    def _animations_enabled():
        """Use native reduced-motion preference where Qt exposes no equivalent."""
        hints = QGuiApplication.styleHints()
        factor = hints.property('animationDurationFactor')
        if factor is not None:
            return factor != 0
        if sys.platform == 'win32':
            try:
                import ctypes
                enabled = ctypes.c_int(1)
                # SPI_GETCLIENTAREAANIMATION reflects Windows accessibility setting.
                if ctypes.windll.user32.SystemParametersInfoW(0x1042, 0, ctypes.byref(enabled), 0):
                    return bool(enabled.value)
            except (AttributeError, OSError):
                pass
        return True

    def close_flyout(self):
        self._hover_timer.stop(); self._leave_timer.stop(); self._animation.stop()
        if self._active is not None:
            card=self.cards[self._active]; self._flyout_layout.removeWidget(card); card.setParent(self._storage); card.hide()
        self._flyout.hide(); self._active=None; self._pinned=False; self._pinned_key=None
    def _schedule_close(self):
        if not self._pinned: self._leave_timer.start(self.tokens['leave_ms'])
    def _maybe_close(self):
        if self._pinned or self._active is None: return
        pos=QCursor.pos()
        button=self.buttons[self._active]
        if button.rect().contains(button.mapFromGlobal(pos)) or self._flyout.rect().contains(self._flyout.mapFromGlobal(pos)): return
        self.close_flyout()
    def eventFilter(self, obj, event):
        if event.type() == QEvent.ContextMenu and obj is not self._flyout:
            self.contextMenuRequested.emit(event.globalPos())
            event.accept()
            return True
        if event.type() == QEvent.Wheel and event.modifiers() & Qt.ControlModifier:
            self._wheel_remainder += event.angleDelta().y()
            while abs(self._wheel_remainder) >= 120:
                step = 1 if self._wheel_remainder > 0 else -1
                self._wheel_remainder -= step*120
                self.scaleStepRequested.emit(step)
            event.accept()
            return True
        if obj is self._flyout:
            if event.type() == QEvent.Enter:
                self._leave_timer.stop()
            elif event.type() == QEvent.Leave:
                self._schedule_close()
        if event.type() == QEvent.KeyPress and event.key() == Qt.Key_Escape:
            self.close_flyout()
            return True
        return super().eventFilter(obj, event)

    def keyPressEvent(self,event):
        if event.key()==Qt.Key_Escape: self.close_flyout()
        else: super().keyPressEvent(event)
    def hideEvent(self,event):
        self.close_flyout(); super().hideEvent(event)
