import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import uuid
from pathlib import Path
from unittest.mock import patch

from PySide6.QtCore import QCoreApplication
from PySide6.QtNetwork import QLocalServer
from PySide6.QtWidgets import QApplication

from system.single_instance import SingleInstance, default_name

APP = QApplication.instance() or QApplication([])
ROOT = Path(__file__).resolve().parents[1]


def unique():
    return f"ClaudeHUDTest-{uuid.uuid4().hex}"


def stop_process(process, interpreter_pid=None):
    """Wait for an owned test launch, including Windows venv redirector children."""
    if os.name == 'nt' and interpreter_pid is not None:
        import _winapi
        try:
            handle = _winapi.OpenProcess(_winapi.SYNCHRONIZE | 0x0001, False, interpreter_pid)
        except OSError as exc:
            if exc.winerror != 87:  # the interpreter may already have exited
                raise
        else:
            try:
                if _winapi.WaitForSingleObject(handle, 0) == _winapi.WAIT_TIMEOUT:
                    _winapi.TerminateProcess(handle, 1)
                if _winapi.WaitForSingleObject(handle, 10000) != _winapi.WAIT_OBJECT_0:
                    raise TimeoutError(f'Test interpreter {interpreter_pid} did not exit')
            finally:
                _winapi.CloseHandle(handle)
    if process.poll() is None:
        if os.name != 'nt' or interpreter_pid is None:
            process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)


def pump(until, timeout=3.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end and not until():
        QCoreApplication.processEvents()
        time.sleep(0.01)
    return until()


IN_PROCESS_PIPE_NOTE = ('on Windows the wake-up never arrives when a process connects to its own named pipe (CI, cause '
                        'unknown); real launches are other processes and are covered by TwoProcessTests / RealMainTests')


def launch_from_another_thread(name):
    """A second launch is another process in real life. Running it in a thread keeps the primary's event loop
    turning while the newcomer blocks on its socket (on Windows' named pipes the in-thread version lost the wake-up)."""
    result = []

    def work():
        result.append(SingleInstance(name).acquire())

    thread = threading.Thread(target=work)
    thread.start()
    pump(lambda: not thread.is_alive(), 10)
    thread.join(5)
    return result[0] if result else None


class SingleInstanceTests(unittest.TestCase):
    def setUp(self):
        self.opened = []

    def tearDown(self):
        for instance in self.opened:
            instance.release()

    def make(self, name):
        instance = SingleInstance(name)
        self.opened.append(instance)
        return instance

    def test_first_is_primary_second_is_not(self):
        name = unique()
        first, second = self.make(name), self.make(name)
        self.assertTrue(first.acquire())
        self.assertTrue(first.is_primary())
        self.assertFalse(second.acquire())
        self.assertFalse(second.is_primary())

    @unittest.skipIf(sys.platform == 'win32', IN_PROCESS_PIPE_NOTE)
    def test_secondary_wakes_the_primary(self):
        name = unique()
        first = self.make(name)
        woken = []
        first.wakeRequested.connect(lambda: woken.append(1))
        self.assertTrue(first.acquire())
        self.assertIs(launch_from_another_thread(name), False)
        self.assertTrue(pump(lambda: len(woken) == 1))

    @unittest.skipIf(sys.platform == 'win32', IN_PROCESS_PIPE_NOTE)
    def test_every_extra_launch_wakes_once(self):
        name = unique()
        first = self.make(name)
        woken = []
        first.wakeRequested.connect(lambda: woken.append(1))
        self.assertTrue(first.acquire())
        for _ in range(3):
            self.assertIs(launch_from_another_thread(name), False)
        self.assertTrue(pump(lambda: len(woken) == 3))
        pump(lambda: False, 0.2)
        self.assertEqual(len(woken), 3)

    def test_unrelated_names_do_not_interfere(self):
        a, b = self.make(unique()), self.make(unique())
        self.assertTrue(a.acquire())
        self.assertTrue(b.acquire())

    def test_a_released_primary_can_be_replaced(self):
        name = unique()
        first = self.make(name)
        self.assertTrue(first.acquire())
        first.release()
        self.assertFalse(first.is_primary())
        self.assertTrue(self.make(name).acquire())

    @unittest.skipIf(sys.platform == 'win32', 'unix socket files only')
    def test_a_stale_socket_file_from_a_crash_does_not_lock_everyone_out(self):
        name = unique()
        probe = QLocalServer()
        probe.listen(name)
        path = probe.fullServerName()
        probe.close()
        Path(path).write_text('left behind by a crash')
        self.assertTrue(os.path.exists(path))
        self.assertTrue(self.make(name).acquire())

    def test_if_the_socket_cannot_be_opened_the_app_still_starts(self):
        with patch.object(QLocalServer, 'listen', return_value=False), self.assertLogs('ClaudeHUD', level='WARNING'):
            instance = self.make(unique())
            self.assertTrue(instance.acquire())

    def test_release_is_safe_to_repeat(self):
        instance = self.make(unique())
        instance.acquire()
        instance.release()
        instance.release()

    def test_default_name_is_per_user_and_not_the_rust_builds(self):
        name = default_name()
        self.assertNotEqual(name, 'ClaudeHUDMonitorSingleInstance')
        self.assertTrue(name.startswith('ClaudeHUDMonitor-Python'))
        with patch('system.single_instance.getpass.getuser', return_value='alice'):
            a = default_name()
        with patch('system.single_instance.getpass.getuser', return_value='bob'):
            b = default_name()
        self.assertNotEqual(a, b)

    def test_default_name_can_be_overridden_for_tests(self):
        with patch.dict(os.environ, {'CLAUDEHUD_SINGLE_INSTANCE_NAME': 'Custom'}):
            self.assertEqual(default_name(), 'Custom')

    def test_default_name_survives_an_unknown_user(self):
        with patch('system.single_instance.getpass.getuser', side_effect=OSError):
            self.assertTrue(default_name().startswith('ClaudeHUDMonitor-Python'))


SCRIPT = """
import os, sys, time
os.environ['QT_QPA_PLATFORM'] = 'offscreen'
sys.path.insert(0, {root!r})
from PySide6.QtCore import QCoreApplication
from system.single_instance import SingleInstance
app = QCoreApplication([])
instance = SingleInstance({name!r})
if not instance.acquire():
    sys.exit(3)
expect = {expect}
woken = []
instance.wakeRequested.connect(lambda: woken.append(1))
print(os.getpid(), flush=True)
print('READY', flush=True)
end = time.monotonic() + 25
while time.monotonic() < end and len(woken) < expect:
    app.processEvents()
    time.sleep(0.01)
settle = time.monotonic() + 0.5          # nothing extra may arrive after the expected wake-ups
while time.monotonic() < settle:
    app.processEvents()
    time.sleep(0.01)
sys.exit(0 if len(woken) == expect else 4)
"""


class TwoProcessTests(unittest.TestCase):
    def _run(self, launches):
        """Start a primary that expects `launches` wake-ups, then launch that many extra copies."""
        name, expect = unique(), launches
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / 'child.py'
            script.write_text(SCRIPT.format(root=str(ROOT), name=name, expect=expect), encoding='utf-8')
            first = subprocess.Popen([sys.executable, '-I', str(script)], stdout=subprocess.PIPE, text=True)
            interpreter_pid = None
            try:
                interpreter_pid = int(first.stdout.readline())
                self.assertEqual(first.stdout.readline().strip(), 'READY')
                for _ in range(launches):
                    second = subprocess.run([sys.executable, '-I', str(script)], timeout=30)
                    self.assertEqual(second.returncode, 3)        # not the primary: asked it to wake, then left
                self.assertEqual(first.wait(timeout=40), 0)       # the primary saw exactly that many wake-ups
            finally:
                try:
                    stop_process(first, interpreter_pid)
                finally:
                    first.stdout.close()

    def test_a_second_process_exits_and_the_first_is_woken(self):
        self._run(1)

    def test_every_extra_process_wakes_the_first_once(self):
        self._run(3)


class RealMainTests(unittest.TestCase):
    """The wiring in main.py: the second real launch exits at once and the first one is told to show itself."""

    def test_second_launch_of_main_exits_and_wakes_the_first(self):
        with tempfile.TemporaryDirectory() as home:
            env = dict(os.environ, QT_QPA_PLATFORM='offscreen', HOME=home, USERPROFILE=home,
                       XDG_CONFIG_HOME=os.path.join(home, '.config'),
                       APPDATA=home, CLAUDEHUD_SINGLE_INSTANCE_NAME=unique())
            env.pop('CLAUDE_CONFIG_DIR', None)
            bootstrap = ("import os, runpy; print(os.getpid(), flush=True); "
                         f"runpy.run_path({str(ROOT / 'main.py')!r}, run_name='__main__')")
            cmd = [sys.executable, '-B', '-c', bootstrap]
            first = subprocess.Popen(cmd, cwd=str(ROOT), env=env, stdout=subprocess.PIPE,
                                     stderr=subprocess.DEVNULL, text=True)
            interpreter_pid = None
            try:
                interpreter_pid = int(first.stdout.readline())
                logs = lambda: ''.join(p.read_text(errors='replace') for p in Path(home).rglob('*.log'))
                end = time.monotonic() + 30
                while time.monotonic() < end and 'HUD window created and shown' not in logs():
                    self.assertIsNone(first.poll(), 'the first launch died: ' + logs()[-800:])
                    time.sleep(0.2)
                self.assertIn('HUD window created and shown', logs())
                second = subprocess.run(cmd, cwd=str(ROOT), env=env, timeout=30,
                                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                self.assertEqual(second.returncode, 0)
                end = time.monotonic() + 10
                while time.monotonic() < end and 'asked this one to show itself' not in logs():
                    time.sleep(0.1)
                self.assertIn('asked this one to show itself', logs())
                self.assertIsNone(first.poll())               # the first keeps running
                self.assertEqual(logs().count('HUD window created and shown'), 1)
            finally:
                try:
                    stop_process(first, interpreter_pid)
                finally:
                    first.stdout.close()


if __name__ == '__main__':
    unittest.main()
