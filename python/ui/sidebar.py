"""Compact provider navigation and reusable card flyouts."""
import math
import sys
from PySide6.QtCore import Qt, Signal, QSize, QRectF, QPoint, QTimer, QEvent, QPropertyAnimation, QEasingCurve
from PySide6.QtGui import QPainter, QColor, QPen, QFont, QGuiApplication, QCursor
from PySide6.QtWidgets import QWidget, QPushButton, QVBoxLayout, QHBoxLayout, QFrame, QSizePolicy
from ui.provider_card import PROVIDER_THEMES
from ui.styles import get_progress_color, get_cards_stylesheet

SIDEBAR_TOKENS = {
    'dark': dict(background='#1e1e24', text='#f2f2f7', muted='#a6a6b3', track='#454550', hover='#30303a', pressed='#40404c', focus='#38bdf8', separator='#454550', stale='#f59e0b', providers={k: v['color'] for k, v in PROVIDER_THEMES.items()}, font='Segoe UI', font_size=12, spacing=4, radius=6, hover_ms=300, leave_ms=600, animation_ms=160),
    'light': dict(background='#f6f4fa', text='#1f1f24', muted='#555563', track='#ceced8', hover='#e6e4ed', pressed='#d8d6e1', focus='#0369a1', separator='#ceced8', stale='#92400e', providers={'claude':'#0369a1','agy':'#047857','codex':'#7e22ce'}, font='Segoe UI', font_size=12, spacing=4, radius=6, hover_ms=300, leave_ms=600, animation_ms=160),
}
STRIP_WIDTH = 92
PANEL_WIDTH = 220
_NAMES = {'claude':'Claude', 'agy':'AGY', 'codex':'Codex'}

class _Button(QPushButton):
    def __init__(self, kind, owner):
        super().__init__(owner)
        self.kind, self.owner = kind, owner
        self.value, self.stale, self.offline = None, False, False
        self.display_text = '--'
        self.setFixedSize(92, 84) if kind in _NAMES else self.setFixedSize(40, 40)
        self.setFocusPolicy(Qt.StrongFocus)
        self.setMouseTracking(True)
        self.setAccessibleName(_NAMES.get(kind, 'Toggle sidebar' if kind == 'toggle' else 'Settings'))
        self.setToolTip(self.accessibleName())
    def keyPressEvent(self, event):
        if event.key() in (Qt.Key_Return, Qt.Key_Enter):
            self.click()
        elif event.key() == Qt.Key_Escape:
            self.owner.close_flyout()
        else:
            super().keyPressEvent(event)
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
        t = self.owner.tokens
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(t['pressed'] if self.isDown() else t['hover'] if self.underMouse() else t['background']))
        p.drawRoundedRect(QRectF(self.rect()).adjusted(2,2,-2,-2), t['radius'], t['radius'])
        if self.hasFocus():
            p.setBrush(Qt.NoBrush); p.setPen(QPen(QColor(t['focus']), 2))
            p.drawRoundedRect(QRectF(self.rect()).adjusted(3,3,-3,-3), t['radius'], t['radius'])
        p.setPen(QPen(QColor(t['text']), 1.6))
        if self.kind == 'toggle':
            p.drawRect(12,12,16,16); p.drawLine(22,12,22,28)
            return
        if self.kind == 'settings':
            p.drawEllipse(12,12,16,16); p.drawEllipse(17,17,6,6)
            for a in range(0,360,45):
                r=math.radians(a); p.drawLine(QPoint(round(20+9*math.cos(r)),round(20+9*math.sin(r))),QPoint(round(20+12*math.cos(r)),round(20+12*math.sin(r))))
            return
        ring = QRectF(28,6,36,36)
        p.setPen(QPen(QColor(t['track']), 2)); p.setBrush(Qt.NoBrush); p.drawEllipse(ring)
        if self.value is not None and not self.offline:
            p.setPen(QPen(QColor(get_progress_color(self.value,self.owner.dark)),2))
            p.drawArc(ring,90*16,-round(self.value*3.6*16))
        p.setPen(QPen(QColor(t['muted'] if self.offline else t['providers'][self.kind]),2))
        if self.kind == 'claude':
            for a in range(0,360,45):
                r=math.radians(a); p.drawLine(QPoint(round(46+4*math.cos(r)),round(24+4*math.sin(r))),QPoint(round(46+11*math.cos(r)),round(24+11*math.sin(r))))
        elif self.kind == 'agy':
            p.drawLine(36,31,46,16); p.drawLine(46,16,56,31); p.drawLine(40,27,52,27)
        else:
            p.drawLine(42,17,36,24); p.drawLine(36,24,42,31); p.drawLine(50,17,56,24); p.drawLine(56,24,50,31)
        p.setFont(QFont(t['font'], t['font_size']-3)); p.setPen(QColor(t['text']))
        p.drawText(QRectF(0,45,92,18),Qt.AlignCenter,_NAMES[self.kind])
        p.setPen(QColor(t['muted'] if self.offline else t['text']))
        p.drawText(QRectF(0,63,92,18),Qt.AlignCenter,self.display_text)
        if self.stale:
            p.setPen(Qt.NoPen); p.setBrush(QColor(t['stale'])); p.drawEllipse(68,9,6,6)

class SidebarWidget(QWidget):
    stateChangeRequested = Signal(str)
    settingsRequested = Signal()
    def __init__(self, cards: dict, parent=None):
        super().__init__(parent)
        self.cards = dict(cards)
        self._state, self.dark = 'strip', True
        self.tokens = SIDEBAR_TOKENS['dark']
        self._active, self._pinned = None, False
        self._hover_id = None
        self._hover_timer = QTimer(self); self._hover_timer.setSingleShot(True)
        self._hover_timer.timeout.connect(lambda: self._open_flyout(self._hover_id))
        self._leave_timer = QTimer(self); self._leave_timer.setSingleShot(True)
        self._leave_timer.timeout.connect(self._maybe_close)
        self._layout = QVBoxLayout(self); self._layout.setContentsMargins(0,0,0,0); self._layout.setSpacing(0)
        header = QHBoxLayout(); header.setContentsMargins(4,0,4,0); header.setSpacing(4)
        self.toggle_button = _Button('toggle',self); self.settings_button = _Button('settings',self)
        header.addWidget(self.toggle_button); header.addStretch(); header.addWidget(self.settings_button)
        self._layout.addLayout(header)
        self.toggle_button.clicked.connect(lambda: self.stateChangeRequested.emit('panel' if self._state=='strip' else 'strip'))
        self.settings_button.clicked.connect(self.settingsRequested.emit)
        self._strip = QWidget(self); strip_layout = QVBoxLayout(self._strip); strip_layout.setContentsMargins(0,0,0,0); strip_layout.setSpacing(0)
        self.buttons = {}
        for key in _NAMES:
            button = _Button(key,self); self.buttons[key]=button; strip_layout.addWidget(button)
            button.clicked.connect(lambda checked=False,k=key: self._click_provider(k))
        self._layout.addWidget(self._strip)
        self._panel = QWidget(self); self._panel_layout = QVBoxLayout(self._panel); self._panel_layout.setContentsMargins(4,4,4,4); self._panel_layout.setSpacing(4)
        self._layout.addWidget(self._panel); self._panel.hide()
        self._storage = QWidget(self); self._storage.hide()
        for card in self.cards.values():
            card.setParent(self._storage); card.hide()
            card.setMinimumWidth(0); card.setSizePolicy(QSizePolicy.Ignored,QSizePolicy.Preferred)
        self._flyout = QWidget(self,Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint)
        self._flyout.setAttribute(Qt.WA_ShowWithoutActivating)
        self._flyout_layout=QVBoxLayout(self._flyout); self._flyout_layout.setContentsMargins(4,4,4,4)
        self._flyout.installEventFilter(self)
        self._animation=QPropertyAnimation(self._flyout,b'pos',self)
        self._animation.setEasingCurve(QEasingCurve.OutCubic)
        self.set_appearance(True)
    def state(self):
        return self._state
    def set_state(self,state):
        if state not in ('strip','panel'): raise ValueError(state)
        if state==self._state: return
        self.close_flyout(); self._state=state
        self._strip.setVisible(state=='strip'); self._panel.setVisible(state=='panel')
        for key,card in self.cards.items():
            if state=='panel':
                self._panel_layout.addWidget(card); card.show()
            else:
                self._panel_layout.removeWidget(card); card.setParent(self._storage); card.hide()
        if state=='panel':
            for index in range(self._panel_layout.count()-1,0,-1):
                line=QFrame(self._panel); line.setObjectName('SidebarDivider'); line.setFixedHeight(1)
                self._panel_layout.insertWidget(index,line)
        else:
            while self._panel_layout.count():
                item=self._panel_layout.takeAt(0)
                if item.widget(): item.widget().deleteLater()
        self.setFixedWidth(STRIP_WIDTH if state=='strip' else PANEL_WIDTH)
        self.updateGeometry()
    def set_appearance(self,dark):
        self.dark=bool(dark); self.tokens=SIDEBAR_TOKENS['dark' if dark else 'light']; t=self.tokens
        self.setObjectName('Sidebar')
        sheet=get_cards_stylesheet(dark=dark)+f"QWidget#Sidebar {{background:{t['background']};}} QFrame#SidebarDivider {{background:{t['separator']};border:none;}}"
        self.setStyleSheet(sheet)
        self._flyout.setStyleSheet(get_cards_stylesheet(dark=dark)+f"QWidget {{background-color:{t['background']};}}")
        self.setFixedWidth(STRIP_WIDTH if self._state=='strip' else PANEL_WIDTH)
        for button in self.buttons.values(): button.update()
        self.toggle_button.update(); self.settings_button.update()
    def update_metrics(self,data):
        button=self.buttons.get(data.provider_id)
        if button is None: return
        rows=[(data.metric1_title,data.metric1_val),(data.metric2_title,data.metric2_val)]+[(r.title,r.val) for r in data.extra_metrics]
        values=[v for _,v in rows if v is not None]
        button.value=max(values) if values else None
        button.offline=bool(data.error and not data.stale); button.stale=bool(data.stale)
        button.display_text='OFF' if button.offline else '--' if button.value is None else f'{button.value:.0f}%'
        summary='; '.join(f'{name}: {value:.0f}%' if value is not None else f'{name}: --' for name,value in rows)
        button.setToolTip(f"{_NAMES[data.provider_id]} — {summary}"+(' — STALE' if data.stale else '')+(f' — {data.error}' if data.error else ''))
        button.setAccessibleDescription(button.toolTip()); button.update()
    def update_countdown(self):
        pass
    def preferred_size(self):
        self._layout.activate()
        return QSize(STRIP_WIDTH if self._state=='strip' else PANEL_WIDTH,self._layout.sizeHint().height())
    def _hover(self,key):
        self._leave_timer.stop()
        if self._state!='strip' or self._pinned: return
        self._hover_id=key; self._hover_timer.start(self.tokens['hover_ms'])
    def _click_provider(self,key):
        if self._active==key and self._pinned: self.close_flyout(); return
        if self._active != key:
            self.close_flyout()
        self._open_flyout(key); self._pinned=True
    def _open_flyout(self,key):
        if self._state!='strip' or key not in self.cards: return
        if self._pinned and self._active!=key: return
        self.close_flyout(); self._active=key
        card=self.cards[key]; self._flyout_layout.addWidget(card); card.show()
        button=self.buttons[key]; origin=button.mapToGlobal(QPoint(0,button.height()))
        screen=button.screen(); area=screen.availableGeometry()
        height=min(self._flyout_layout.sizeHint().height(),area.height())
        self._flyout.setFixedSize(260,height)
        target=QPoint(max(area.left(),min(origin.x()-260,area.right()-259)),max(area.top(),min(origin.y()-height,area.bottom()-height+1)))
        motion = self._animations_enabled()
        self._flyout.move(target); self._flyout.show()
        if motion:
            self._animation.setDuration(self.tokens['animation_ms']); self._animation.setStartValue(target+QPoint(12,0)); self._animation.setEndValue(target); self._animation.start()
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
        self._flyout.hide(); self._active=None; self._pinned=False
    def _schedule_close(self):
        if not self._pinned: self._leave_timer.start(self.tokens['leave_ms'])
    def _maybe_close(self):
        if self._pinned or self._active is None: return
        pos=QCursor.pos()
        button=self.buttons[self._active]
        if button.rect().contains(button.mapFromGlobal(pos)) or self._flyout.rect().contains(self._flyout.mapFromGlobal(pos)): return
        self.close_flyout()
    def eventFilter(self,obj,event):
        if obj is self._flyout:
            if event.type()==QEvent.Enter: self._leave_timer.stop()
            elif event.type()==QEvent.Leave: self._schedule_close()
            elif event.type()==QEvent.KeyPress and event.key()==Qt.Key_Escape: self.close_flyout(); return True
        return super().eventFilter(obj,event)
    def keyPressEvent(self,event):
        if event.key()==Qt.Key_Escape: self.close_flyout()
        else: super().keyPressEvent(event)
    def hideEvent(self,event):
        self.close_flyout(); super().hideEvent(event)
