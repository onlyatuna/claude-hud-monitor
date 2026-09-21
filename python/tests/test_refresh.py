import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import threading
import time
import unittest
from PySide6.QtWidgets import QApplication
from core.refresh_controller import RefreshController
from core.providers.base import UsageMetrics

APP = QApplication.instance() or QApplication([])

def wait_for(predicate):
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        APP.processEvents()
        if predicate():
            return
        time.sleep(0.005)
    raise AssertionError('worker did not complete')

class Provider:
    def __init__(self):
        self.calls = 0
        self.release = threading.Event()
        self.release.set()
        self.error = None
    def fetch_usage(self):
        self.calls += 1
        number = self.calls
        self.release.wait(2)
        return UsageMetrics(provider_id='test', metric1_val=number, error=self.error)

class RefreshTests(unittest.TestCase):
    def setUp(self):
        self.provider = Provider()
        self.now = 0
        self.controller = RefreshController({'test': self.provider}, clock=lambda: self.now)
        self.results = []
        self.controller.updated.connect(self.results.append)
    def tearDown(self):
        self.provider.release.set()
        self.controller.stop()
    def test_wake_discards_old_result_without_overlap(self):
        self.provider.release.clear()
        self.controller.refresh()
        wait_for(lambda: self.provider.calls == 1)
        self.controller.refresh()
        self.controller.refresh()
        self.assertEqual(self.provider.calls, 1)
        self.provider.release.set()
        wait_for(lambda: len(self.results) == 1)
        self.assertEqual(self.provider.calls, 2)
        self.assertEqual(self.results[0].metric1_val, 2)
    def test_failure_retains_data_and_backs_off(self):
        self.controller.poll()
        wait_for(lambda: len(self.results) == 1)
        self.provider.error = 'timeout'
        self.now = 60
        self.controller.poll()
        wait_for(lambda: len(self.results) == 2)
        self.assertTrue(self.results[-1].stale)
        self.assertEqual(self.results[-1].metric1_val, 1)
        self.now = 120
        self.controller.poll()
        wait_for(lambda: len(self.results) == 3)
        self.assertEqual(self.controller.states['test'].due, 240)
        self.provider.error = None
        self.now = 240
        self.controller.poll()
        wait_for(lambda: len(self.results) == 4)
        self.assertFalse(self.results[-1].stale)
        self.assertEqual(self.controller.states['test'].failures, 0)
    def test_stop_ignores_late_results(self):
        self.provider.release.clear()
        self.controller.refresh()
        wait_for(lambda: self.provider.calls == 1)
        self.controller.stop()
        self.provider.release.set()
        for _ in range(20):
            APP.processEvents()
            time.sleep(0.005)
        self.assertEqual(self.results, [])

    def test_retry_after_is_finite_and_capped(self):
        for retry_after, expected_due in ((float('nan'), 60), (-1, 60), (float('inf'), 60), (10 ** 20, 86400)):
            self.controller._complete('test', 0, UsageMetrics(provider_id='test', error='limited', retry_after=retry_after))
            self.assertEqual(self.controller.states['test'].due, expected_due)
            self.controller.states['test'].failures = 0

    def test_one_slow_provider_does_not_block_another(self):
        other = Provider()
        controller = RefreshController({'slow': self.provider, 'fast': other})
        results = []
        controller.updated.connect(results.append)
        self.provider.release.clear()
        try:
            controller.refresh()
            wait_for(lambda: len(results) == 1)
            self.assertTrue(controller.states['slow'].running)
            self.assertFalse(controller.states['fast'].running)
        finally:
            self.provider.release.set()
            controller.stop()
