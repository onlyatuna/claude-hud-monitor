"""Install the agy status line hook and read its quota-only cache."""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone

from core.config_manager import get_user_config_dir

HOOK_FILENAME = "agy_statusline_hook.py"
QUOTA_CACHE_FILENAME = "agy-quota.json"

HOOK_SOURCE = '''import json
import os
import sys
import tempfile
import time
from datetime import datetime, timezone

def main():
    temporary = None
    try:
        directory = os.path.dirname(os.path.abspath(__file__))
        target = os.path.join(directory, "agy-quota.json")
        if os.path.exists(target) and time.time() - os.path.getmtime(target) < 2:
            return
        data = json.loads(sys.stdin.read())
        if not isinstance(data, dict) or not isinstance(data.get("quota"), dict):
            return
        quota = {}
        for key, bucket in data["quota"].items():
            if not isinstance(bucket, dict):
                continue
            fraction = bucket.get("remaining_fraction")
            if isinstance(fraction, bool) or not isinstance(fraction, (int, float)) or not 0 <= fraction <= 1:
                continue
            clean = {"remaining_fraction": fraction}
            if isinstance(bucket.get("reset_time"), str):
                clean["reset_time"] = bucket["reset_time"]
            quota[key] = clean
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=directory, delete=False) as stream:
            temporary = stream.name
            json.dump({"written_at": datetime.now(timezone.utc).isoformat(), "quota": quota}, stream)
        os.replace(temporary, target)
        temporary = None
    except Exception:
        pass
    finally:
        if temporary is not None:
            try:
                os.unlink(temporary)
            except Exception:
                pass

main()
print(" ")
'''


def agy_settings_path() -> str:
    return os.path.expanduser('~/.gemini/antigravity-cli/settings.json')


def quota_cache_path() -> str:
    return os.path.join(get_user_config_dir(), QUOTA_CACHE_FILENAME)


def _utc(value):
    if not isinstance(value, str):
        return None
    try:
        parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
        if parsed.tzinfo is None:
            return None
        return parsed.astimezone(timezone.utc)
    except (ValueError, OverflowError):
        return None


def read_quota_cache(path=None, now=None):
    try:
        with open(path if path is not None else quota_cache_path(), encoding='utf-8') as stream:
            data = json.load(stream)
        if not isinstance(data, dict) or not isinstance(data.get('quota'), dict):
            return None
        written_at = _utc(data.get('written_at'))
        if written_at is None:
            return None
        now = now if now is not None else datetime.now(timezone.utc)
        buckets = {}
        for key, bucket in data['quota'].items():
            if not isinstance(bucket, dict):
                continue
            fraction = bucket.get('remaining_fraction')
            if isinstance(fraction, bool) or not isinstance(fraction, (int, float)) or not 0 <= fraction <= 1:
                continue
            reset = _utc(bucket.get('reset_time'))
            if reset is not None and reset <= now:
                continue
            buckets[key] = {'remaining_fraction': float(fraction), 'reset_time': reset}
        return {'written_at': written_at, 'buckets': buckets} if buckets else None
    except Exception:
        return None


def _load_settings(path):
    with open(path, encoding='utf-8') as stream:
        settings = json.load(stream)
    if not isinstance(settings, dict):
        raise ValueError('Settings must be an object')
    return settings


def _owned(statusline):
    return (isinstance(statusline, dict)
            and isinstance(statusline.get('command'), str)
            and HOOK_FILENAME in statusline['command'])


def _backup(path):
    # Exclusive creation preserves earlier backups made within the same second.
    stamp = datetime.now().strftime('%Y%m%d-%H%M%S')
    base = f'{path}.bak-hud-{stamp}'
    candidate = base
    suffix = 0
    while True:
        try:
            with open(path, 'rb') as source, open(candidate, 'xb') as target:
                shutil.copyfileobj(source, target)
            return
        except FileExistsError:
            suffix += 1
            candidate = f'{base}-{suffix}'


def _write_settings(path, settings):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', dir=os.path.dirname(os.path.abspath(path)), delete=False) as stream:
            temporary = stream.name
            stream.write(json.dumps(settings, indent=2, ensure_ascii=False))
        os.replace(temporary, path)
        temporary = None
    finally:
        if temporary is not None:
            os.unlink(temporary)


def cache_written_at(path=None):
    """When the hook last wrote the quota cache (UTC), or None if there is no readable cache yet."""
    try:
        with open(path if path is not None else quota_cache_path(), encoding='utf-8') as stream:
            data = json.load(stream)
        return _utc(data.get('written_at')) if isinstance(data, dict) else None
    except Exception:
        return None


def find_python(platform=None):
    """The command agy will run the hook with ('python' on Windows, 'python3' elsewhere), if it really runs Python.

    Windows ships a Store stub called python.exe that is on the PATH without Python behind it, so the command is
    actually started once instead of only looked up.
    """
    name = 'python' if (platform or sys.platform) == 'win32' else 'python3'
    executable = shutil.which(name)
    if not executable:
        return None
    try:
        result = subprocess.run([executable, '-c', 'import sys; sys.exit(0 if sys.version_info >= (3, 6) else 1)'],
                                capture_output=True, timeout=10, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
    except (OSError, subprocess.SubprocessError):
        return None
    return name if result.returncode == 0 else None


def _needs_quoting(text) -> bool:
    return any(character.isspace() or character in '"\'' for character in text)


def _short_path(path):
    """Windows 8.3 name of an existing directory (GetShortPathNameW), or None."""
    try:
        import ctypes
        buffer = ctypes.create_unicode_buffer(1024)
        length = ctypes.windll.kernel32.GetShortPathNameW(os.path.abspath(path), buffer, len(buffer))
        return buffer.value if 0 < length < len(buffer) else None
    except Exception:
        return None


def _command_hook_path(hook_path):
    """The hook path as it can appear in the command (agy's parsing of quotes is not known, so none is used).

    A path with spaces (a Windows user name such as 'John Smith') is written with its 8.3 directory name when the
    volume has one; otherwise there is no safe spelling and the caller reports 'unsupported_path'.
    """
    if not _needs_quoting(hook_path):
        return hook_path
    if sys.platform != 'win32':
        return None
    directory = os.path.dirname(hook_path)
    os.makedirs(directory, exist_ok=True)           # a short name only exists for a directory that exists
    short = _short_path(directory)
    candidate = os.path.join(short, os.path.basename(hook_path)) if short else None
    return candidate if candidate and not _needs_quoting(candidate) else None


def _hook_current(hook_path) -> bool:
    try:
        with open(hook_path, encoding='utf-8') as stream:
            return stream.read() == HOOK_SOURCE
    except OSError:
        return False


def _points_at(statusline, command_path) -> bool:
    command = statusline.get('command', '')
    tokens = command.split()
    return (statusline.get('type') == 'command' and statusline.get('enabled') is True and bool(command_path)
            and bool(tokens) and os.path.normcase(os.path.abspath(tokens[-1])) == os.path.normcase(os.path.abspath(command_path)))


def statusline_status(settings_path=None, config_dir=None) -> str:
    """'installed', 'needs_update' (ours but stale or pointing elsewhere), 'not_installed', 'conflict' (someone
    else's statusLine), 'agy_missing' (no agy settings file) or 'error'. Changes nothing."""
    try:
        settings_path = settings_path if settings_path is not None else agy_settings_path()
        if not os.path.isfile(settings_path):
            return 'agy_missing'
        settings = _load_settings(settings_path)
        if 'statusLine' not in settings:
            return 'not_installed'
        if not _owned(settings['statusLine']):
            return 'conflict'
        config_dir = config_dir if config_dir is not None else get_user_config_dir()
        hook_path = os.path.abspath(os.path.join(config_dir, HOOK_FILENAME))
        if not _hook_current(hook_path) or not _points_at(settings['statusLine'], _command_hook_path(hook_path)):
            return 'needs_update'
        return 'installed'
    except Exception:
        return 'error'


def install_statusline(settings_path=None, config_dir=None, python_cmd=None):
    """Enable the hook. Returns 'installed', 'updated' (ours, brought up to date), 'already', or a failure code
    ('conflict', 'unsupported_path', 'agy_missing', 'python_missing', 'error'). Nothing is written on failure."""
    try:
        config_dir = config_dir if config_dir is not None else get_user_config_dir()
        hook_path = os.path.abspath(os.path.join(config_dir, HOOK_FILENAME))
        if python_cmd is not None and any(character in '"\'' for character in python_cmd):
            return 'unsupported_path'
        command_path = _command_hook_path(hook_path)
        if command_path is None:
            return 'unsupported_path'
        settings_path = settings_path if settings_path is not None else agy_settings_path()
        if not os.path.isfile(settings_path):
            return 'agy_missing'
        settings = _load_settings(settings_path)
        statusline = settings.get('statusLine')
        if 'statusLine' in settings and not _owned(statusline):
            return 'conflict'
        ours_before = 'statusLine' in settings
        settings_ok = ours_before and _points_at(statusline, command_path)
        hook_ok = _hook_current(hook_path)
        if settings_ok and hook_ok:
            return 'already'
        if not settings_ok:
            python_cmd = python_cmd if python_cmd is not None else find_python()
            if python_cmd is None:
                return 'python_missing'
            _backup(settings_path)
        os.makedirs(config_dir, exist_ok=True)
        with open(hook_path, 'w', encoding='utf-8') as stream:
            stream.write(HOOK_SOURCE)
        if not settings_ok:                                # a stale hook file alone leaves the settings untouched
            settings['statusLine'] = {'type': 'command', 'command': f'{python_cmd} {command_path}', 'enabled': True}
            _write_settings(settings_path, settings)
        return 'updated' if ours_before else 'installed'
    except Exception:
        return 'error'


def uninstall_statusline(settings_path=None, config_dir=None):
    try:
        settings_path = settings_path if settings_path is not None else agy_settings_path()
        if not os.path.isfile(settings_path):
            return 'not_installed'
        settings = _load_settings(settings_path)
        if 'statusLine' not in settings:
            return 'not_installed'
        if not _owned(settings['statusLine']):
            return 'conflict'
        _backup(settings_path)
        del settings['statusLine']
        _write_settings(settings_path, settings)
        return 'removed'
    except Exception:
        return 'error'


RESULT_MESSAGES = {
    'installed': '已安裝 agy statusLine（請重開 agy 互動模式）',
    'updated': '已更新 agy statusLine 掛鉤（請重開 agy 互動模式）',
    'already': '已安裝，無需變更',
    'conflict': 'agy 已設定其他 statusLine，未變更',
    'unsupported_path': '設定資料夾路徑含空白，無法安裝',
    'agy_missing': '找不到 agy 設定檔，請先安裝並執行過 agy',
    'python_missing': '找不到可用的 python（需要 PATH 上有 python 或 python3），未變更',
    'error': '無法讀寫 agy 設定',
    'removed': '已移除 agy statusLine',
    'not_installed': '尚未安裝',
}
SUCCESS_RESULTS = ('installed', 'updated', 'already', 'removed', 'not_installed')

STATUS_LABELS = {
    'installed': '狀態：已啟用',
    'needs_update': '狀態：需要更新',
    'not_installed': '狀態：未啟用',
    'conflict': '狀態：agy 已使用其他 statusLine（不會覆蓋）',
    'agy_missing': '狀態：找不到 agy 設定檔',
    'error': '狀態：無法讀取 agy 設定',
}


def cli(argv) -> int | None:
    if '--install-agy-statusline' in argv:
        result = install_statusline()
    elif '--uninstall-agy-statusline' in argv:
        result = uninstall_statusline()
    else:
        return None
    print(RESULT_MESSAGES[result])
    return 0 if result in SUCCESS_RESULTS else 1
