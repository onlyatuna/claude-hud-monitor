"""Per-provider scheduling. All mutable state belongs to the Qt UI thread."""
import logging
import math
import random
import threading
import time
import queue
from dataclasses import dataclass, replace
from datetime import datetime, timezone
from PySide6.QtCore import QObject, QTimer, Signal, Slot
from core.providers.base import UsageMetrics


@dataclass
class ProviderState:
    generation: int = 0
    running: bool = False
    pending: bool = False
    failures: int = 0
    due: float = 0.0
    cached: object = None


AUTH_ERROR = "auth"   # 401/403, missing or expired credentials: shown as OFF, never as amber STALE


class RefreshController(QObject):
    updated = Signal(object)
    busy_changed = Signal(bool)

    def __init__(self, providers, interval=60, parent=None, clock=time.monotonic, jitter=0.0, rand=random.uniform):
        super().__init__(parent)
        self.providers = providers
        self.interval = max(20, interval)
        self.clock = clock
        self.jitter, self._rand = jitter, rand   # +-fraction applied to every schedule; 0 = exact (tests)
        self.states = {pid: ProviderState() for pid in providers}
        self._floor = 0          # first generation of an instance added later: above every removed one
        self._born = {}          # id -> first generation of the current incarnation; older results are dropped
        self.closed = False
        self._results = queue.SimpleQueue()
        self.result_timer = QTimer(self)
        self.result_timer.setInterval(25)
        self.result_timer.timeout.connect(self._drain_results)
        self.result_timer.start()
        self.timer = QTimer(self)
        self.timer.setInterval(1000)
        self.timer.timeout.connect(self.poll)

    def start(self):
        self.timer.start()
        self.poll()

    def _delay(self, seconds):
        """`seconds` spread by +-jitter so instances never poll in lockstep; never below the 20 s minimum."""
        if not self.jitter:
            return seconds
        return max(20, seconds * (1 + self._rand(-self.jitter, self.jitter)))

    def set_interval(self, seconds):
        self.interval = max(20, seconds)
        for state in self.states.values():
            if not state.failures:
                state.due = self.clock() + self._delay(self.interval)

    def add_provider(self, pid, provider):
        """Register an instance and fetch it at once. The state exists before the first launch."""
        self.providers[pid] = provider
        self.states[pid] = ProviderState(generation=self._floor)
        self._born[pid] = self._floor
        if not self.closed:
            self._launch(pid)

    def remove_provider(self, pid):
        """Forget an instance; a worker still running for it finishes and its result is dropped."""
        state = self.states.pop(pid, None)
        self.providers.pop(pid, None)
        self._born.pop(pid, None)
        if state is None:
            return
        self._floor = max(self._floor, state.generation + 1)
        self.busy_changed.emit(any(s.running for s in self.states.values()))

    def poll(self):
        if self.closed:
            return
        for pid, state in self.states.items():
            if not state.running and self.clock() >= state.due:
                self._launch(pid)

    def refresh(self):
        """Invalidate old results without spawning overlapping provider processes."""
        if self.closed:
            return
        for pid, state in self.states.items():
            if state.running:
                state.generation += 1
                state.pending = True
            else:
                self._launch(pid)

    def _launch(self, pid):
        state = self.states[pid]
        state.running = True
        state.pending = False
        state.generation += 1
        generation = state.generation
        self.busy_changed.emit(True)
        provider = self.providers[pid]
        results = self._results
        def work():
            try:
                result = provider.fetch_usage()
            except Exception as exc:
                logging.getLogger(__name__).warning("provider=%s failure=%s", pid, type(exc).__name__)
                result = UsageMetrics(provider_id=pid, error="查詢失敗，請查看診斷紀錄", error_code="unexpected")
            # The worker owns no Qt object; safe even after the UI is destroyed.
            results.put((pid, generation, result))
        threading.Thread(target=work, name=f"quota-{pid}", daemon=True).start()

    def _drain_results(self):
        while not self.closed:
            try:
                item = self._results.get_nowait()
            except queue.Empty:
                return
            self._complete(*item)

    @Slot(str, int, object)
    def _complete(self, pid, generation, result):
        if self.closed:
            return
        state = self.states.get(pid)
        if state is None or generation < self._born.get(pid, 0):
            return   # a removed instance (or an earlier incarnation of a re-added id): drop the result
        state.running = False
        if generation != state.generation or state.pending:
            self._launch(pid)
            return
        now = self.clock()
        if result.error:
            state.failures += 1
            delay = min(900, self.interval * 2 ** min(state.failures - 1, 4))
            retry_after = result.retry_after
            valid_retry_after = (isinstance(retry_after, (int, float)) and not isinstance(retry_after, bool)
                                 and math.isfinite(retry_after) and retry_after >= 0)
            if valid_retry_after:
                delay = min(86400, max(delay, retry_after))
            delay = self._delay(delay)
            if valid_retry_after:
                delay = min(86400, max(delay, retry_after))
            logging.getLogger(__name__).warning("provider=%s error=%s retry_seconds=%.1f", pid, result.error_code or "unknown", delay)
            state.due = now + delay
            if result.error_code == AUTH_ERROR:
                state.cached = None     # terminal: the old values are no longer this account's truth, show OFF
            elif state.cached is not None:
                result = replace(state.cached, error=result.error, error_code=result.error_code,
                                 retry_after=result.retry_after, stale=True)
        else:
            state.failures = 0
            state.due = now + self._delay(self.interval)
            result = replace(result, last_success=datetime.now(timezone.utc), stale=False)
            state.cached = result
        self.updated.emit(result)
        self.busy_changed.emit(any(s.running for s in self.states.values()))

    def stop(self):
        self.closed = True
        self.timer.stop()
        self.result_timer.stop()
        for state in self.states.values():
            state.generation += 1
