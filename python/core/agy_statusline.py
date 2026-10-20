"""Install the agy status line hook and read its quota-only cache."""

import json
import os
import shutil
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


def install_statusline(settings_path=None, config_dir=None, python_cmd=None):
    try:
        config_dir = config_dir if config_dir is not None else get_user_config_dir()
        hook_path = os.path.abspath(os.path.join(config_dir, HOOK_FILENAME))
        python_cmd = python_cmd if python_cmd is not None else ('python' if sys.platform == 'win32' else 'python3')
        if any(character.isspace() or character in '\"\'' for character in hook_path) or any(character in '\"\'' for character in python_cmd):
            return 'unsupported_path'
        settings_path = settings_path if settings_path is not None else agy_settings_path()
        settings = _load_settings(settings_path)
        desired = {'type': 'command', 'command': f'{python_cmd} {hook_path}', 'enabled': True}
        if 'statusLine' in settings:
            if not _owned(settings['statusLine']):
                return 'conflict'
            if settings['statusLine'] == desired:
                return 'already'
        _backup(settings_path)
        os.makedirs(config_dir, exist_ok=True)
        with open(hook_path, 'w', encoding='utf-8') as stream:
            stream.write(HOOK_SOURCE)
        settings['statusLine'] = desired
        _write_settings(settings_path, settings)
        return 'installed'
    except Exception:
        return 'error'


def uninstall_statusline(settings_path=None, config_dir=None):
    try:
        settings_path = settings_path if settings_path is not None else agy_settings_path()
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


def cli(argv) -> int | None:
    if '--install-agy-statusline' in argv:
        result = install_statusline()
    elif '--uninstall-agy-statusline' in argv:
        result = uninstall_statusline()
    else:
        return None
    messages = {
        'installed': '已安裝 agy statusLine（請重開 agy 互動模式）',
        'already': '已安裝，無需變更',
        'conflict': 'agy 已設定其他 statusLine，未變更',
        'unsupported_path': '設定資料夾路徑含空白，無法安裝',
        'error': '無法讀寫 agy 設定',
        'removed': '已移除 agy statusLine',
        'not_installed': '尚未安裝',
    }
    print(messages[result])
    return 0 if result in ('installed', 'already', 'removed', 'not_installed') else 1
