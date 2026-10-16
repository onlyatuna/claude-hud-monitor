from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QProgressBar, QFrame, QSizePolicy
)
from PySide6.QtCore import Qt

from core.providers.base import UsageMetrics, BaseProvider
from ui.styles import get_progress_color

PROVIDER_THEMES = {
    "claude": {"color": "#38bdf8", "name": "CLAUDE CODE"},
    "agy": {"color": "#10b981", "name": "ANTIGRAVITY"},
    "codex": {"color": "#a855f7", "name": "OPENAI CODEX"}
}

class ProviderCardWidget(QWidget):
    def __init__(self, provider_id: str, parent=None):
        super().__init__(parent)
        self.provider_id = provider_id
        self.dark = True
        self.theme = PROVIDER_THEMES.get(provider_id, {"color": "#38bdf8", "name": provider_id.upper()})
        self.current_metrics = UsageMetrics(provider_id=provider_id)
        self._init_ui()

    def _init_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(6, 4, 6, 4)
        layout.setSpacing(5)

        # 1. Header: Status dot + Provider Title + Badge
        header = QHBoxLayout()
        header.setSpacing(4)

        self.dot = QLabel("●")
        self.dot.setStyleSheet(f"color: {self.theme['color']}; font-size: 10px;")
        header.addWidget(self.dot)

        self.title = QLabel(self.theme["name"])
        self.title.setStyleSheet(f"color: {self.theme['color']}; font-size: 9.5px; font-weight: 800; letter-spacing: 0.4px;")
        self.title.setSizePolicy(QSizePolicy.Policy.Minimum, QSizePolicy.Policy.Preferred)
        header.addWidget(self.title)

        header.addStretch()

        self.badge = QLabel("--")
        self.badge.setObjectName("Badge")
        header.addWidget(self.badge)

        self.badge2 = QLabel("")
        self.badge2.setObjectName("Badge")
        self.badge2.setVisible(False)
        header.addWidget(self.badge2)

        layout.addLayout(header)

        # 2. Metric 1 (Session 5H)
        m1_box = QVBoxLayout()
        m1_box.setSpacing(1)

        m1_hdr = QHBoxLayout()
        self.m1_label = QLabel("SESSION 5H")
        self.m1_label.setObjectName("MetricTitle")
        m1_hdr.addWidget(self.m1_label)
        m1_hdr.addStretch()
        self.m1_val = QLabel("--")
        self.m1_val.setObjectName("MetricValue")
        self.m1_val.setStyleSheet("font-size: 14px;")
        m1_hdr.addWidget(self.m1_val)
        m1_box.addLayout(m1_hdr)

        self.m1_bar = QProgressBar()
        self.m1_bar.setRange(0, 100)
        self.m1_bar.setValue(0)
        self.m1_bar.setTextVisible(False)
        m1_box.addWidget(self.m1_bar)

        self.m1_sub = QLabel("重設於: --")
        self.m1_sub.setObjectName("SubDetail")
        m1_box.addWidget(self.m1_sub)

        layout.addLayout(m1_box)

        # 3. Metric 2 (Weekly 7D or Credits)
        m2_box = QVBoxLayout()
        m2_box.setSpacing(1)

        m2_hdr = QHBoxLayout()
        self.m2_label = QLabel("WEEKLY 7D")
        self.m2_label.setObjectName("MetricTitle")
        m2_hdr.addWidget(self.m2_label)
        m2_hdr.addStretch()
        self.m2_val = QLabel("--")
        self.m2_val.setObjectName("MetricValue")
        self.m2_val.setStyleSheet("font-size: 14px;")
        m2_hdr.addWidget(self.m2_val)
        m2_box.addLayout(m2_hdr)

        self.m2_bar = QProgressBar()
        self.m2_bar.setRange(0, 100)
        self.m2_bar.setValue(0)
        self.m2_bar.setTextVisible(False)
        m2_box.addWidget(self.m2_bar)

        self.m2_sub = QLabel("重設於: --")
        self.m2_sub.setObjectName("SubDetail")
        m2_box.addWidget(self.m2_sub)

        layout.addLayout(m2_box)
        self.extra_rows = []
        self.extra_container = None

    def _create_extra_row(self):
        if self.extra_container is None:
            self.extra_container = QWidget()
            extra_layout = QVBoxLayout(self.extra_container)
            extra_layout.setContentsMargins(0, 0, 0, 0)
            extra_layout.setSpacing(1)
            self.layout().addWidget(self.extra_container)
        widget = QWidget()
        box = QVBoxLayout(widget)
        box.setContentsMargins(0, 0, 0, 0)
        box.setSpacing(0)
        header = QHBoxLayout()
        header.setContentsMargins(0, 0, 0, 0)
        header.setSpacing(1)
        title = QLabel()
        title.setObjectName("MetricTitle")
        value = QLabel()
        value.setObjectName("MetricValue")
        header.addWidget(title)
        header.addStretch()
        header.addWidget(value)
        box.addLayout(header)
        bar = QProgressBar()
        bar.setRange(0, 100)
        bar.setTextVisible(False)
        box.addWidget(bar)
        sub = QLabel()
        sub.setObjectName("SubDetail")
        box.addWidget(sub)
        self.extra_container.layout().addWidget(widget)
        row = (widget, title, value, bar, sub)
        self.extra_rows.append(row)
        return row

    def _update_extra_metrics(self, metrics):
        while len(self.extra_rows) < len(metrics):
            self._create_extra_row()
        if self.extra_container is not None:
            self.extra_container.setVisible(bool(metrics))
        for index, (widget, title, value, bar, sub) in enumerate(self.extra_rows):
            widget.setVisible(index < len(metrics))
            if index >= len(metrics):
                continue
            metric = metrics[index]
            title.setText(metric.title)
            value.setText(metric.text)
            color = get_progress_color(metric.val, self.dark) if metric.val is not None else "#64748b"
            value.setStyleSheet(f"color: {color}; font-size: 14px;")
            bar.setStyleSheet(f"QProgressBar::chunk {{ background-color: {color}; }}")
            bar.setValue(int(min(100, max(0, metric.val or 0))))
            sub.setStyleSheet("color: #64748b;")
            sub.setText(f"重設於: {BaseProvider.format_countdown(metric.reset)}" if metric.reset else (metric.subtext or "重設於: --"))

    def set_appearance(self, dark: bool):
        self.dark = dark
        base = PROVIDER_THEMES.get(self.provider_id, {"color": "#38bdf8", "name": self.provider_id.upper()})
        self.theme = dict(base)
        if not dark:
            self.theme["color"] = {"claude": "#0369a1", "agy": "#047857", "codex": "#7e22ce"}.get(self.provider_id, "#0369a1")
        self.title.setStyleSheet(f"color: {self.theme['color']}; font-size: 9.5px; font-weight: 800; letter-spacing: 0.4px;")
        self.update_metrics(self.current_metrics)

    def update_metrics(self, data: UsageMetrics):
        self.current_metrics = data
        self._update_extra_metrics([] if data.error and not data.stale else data.extra_metrics)
        if data.provider_name:
            self.title.setText(data.provider_name)
        else:
            self.title.setText(self.theme["name"])

        self.setToolTip(data.error or "")
        error_color = "#ef4444" if self.dark else "#b91c1c"
        if data.error and not data.stale:
            self.dot.setStyleSheet(f"color: {error_color}; font-size: 10px;")
            self.m1_val.setText("ERR")
            self.m1_val.setStyleSheet(f"color: {error_color}; font-size: 13px;")
            self.m1_bar.setValue(0)
            self.m1_sub.setText(data.error.split("\n")[0])
            self.m1_sub.setStyleSheet(f"color: {error_color};")

            self.m2_val.setText("--")
            self.m2_bar.setValue(0)
            self.m2_sub.setText("")
            self.badge.setText("OFFLINE")
            self.badge.setVisible(True)
            self.badge2.setVisible(False)
            return

        self.dot.setStyleSheet(f"color: {self.theme['color']}; font-size: 10px;")
        self.m1_sub.setStyleSheet("color: #64748b;")
        self.m2_sub.setStyleSheet("color: #64748b;")

        # Reset sub labels before updating to prevent residual error messages
        if data.metric1_subtext:
            self.m1_sub.setText(data.metric1_subtext)
        elif not data.metric1_reset:
            self.m1_sub.setText("重設於: --")

        if data.metric2_subtext:
            self.m2_sub.setText(data.metric2_subtext)
        elif not data.metric2_reset:
            self.m2_sub.setText("重設於: --")

        # Metric 1
        self.m1_label.setText(data.metric1_title)
        self.m1_val.setText(data.metric1_text)
        c1 = get_progress_color(data.metric1_val, self.dark) if data.metric1_val is not None else "#64748b"
        self.m1_val.setStyleSheet(f"color: {c1}; font-size: 14px;")
        self.m1_bar.setStyleSheet(f"QProgressBar::chunk {{ background-color: {c1}; }}")
        self.m1_bar.setValue(int(min(100, max(0, data.metric1_val or 0))))

        # Metric 2
        self.m2_label.setText(data.metric2_title)
        self.m2_val.setText(data.metric2_text)
        c2 = get_progress_color(data.metric2_val, self.dark) if data.metric2_val is not None else "#64748b"
        self.m2_val.setStyleSheet(f"color: {c2}; font-size: 14px;")
        self.m2_bar.setStyleSheet(f"QProgressBar::chunk {{ background-color: {c2}; }}")
        self.m2_bar.setValue(int(min(100, max(0, data.metric2_val or 0))))

        # Badges (compact formatting to prevent squeezing title)
        b1 = (data.badge1_text or "").replace(" 剩餘:", ":").replace("剩餘:", ":")
        b2 = (data.badge2_text or "").replace(" 剩餘:", ":").replace("剩餘:", ":")
        if b1 and b2:
            self.badge.setText(b1)
            self.badge.setVisible(True)
            self.badge2.setText(b2)
            self.badge2.setVisible(True)
        elif b1:
            self.badge.setText(b1)
            self.badge.setVisible(True)
            self.badge2.setVisible(False)
        elif b2:
            self.badge.setText(b2)
            self.badge.setVisible(True)
            self.badge2.setVisible(False)
        else:
            self.badge.setText("--")
            self.badge.setVisible(True)
            self.badge2.setVisible(False)

        self.update_countdown()
        if data.stale:
            self.dot.setStyleSheet(f"color: {'#f59e0b' if self.dark else '#92400e'}; font-size: 10px;")
            self.badge.setText("STALE")

    def update_countdown(self):
        data = self.current_metrics
        if not data or (data.error and not data.stale):
            return
        for index in (1, 2):
            label = getattr(self, f"m{index}_sub")
            reset = getattr(data, f"metric{index}_reset")
            subtext = getattr(data, f"metric{index}_subtext")
            label.setText(f"重設於: {BaseProvider.format_countdown(reset)}" if reset else (subtext or "重設於: --"))
        for metric, row in zip(data.extra_metrics, self.extra_rows):
            row[4].setText(f"重設於: {BaseProvider.format_countdown(metric.reset)}" if metric.reset else (metric.subtext or "重設於: --"))
        if data.stale:
            stamp = data.last_success.astimezone().strftime("%m/%d %H:%M:%S") if data.last_success else "--"
            self.m1_sub.setText(f"舊資料 {stamp}")
            self.m1_sub.setStyleSheet(f"color: {'#f59e0b' if self.dark else '#92400e'};")

