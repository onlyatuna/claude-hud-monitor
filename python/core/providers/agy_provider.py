from dataclasses import dataclass
import base64
import json
import os
import subprocess
import sys
import logging
import time
from datetime import datetime, timezone
from typing import Optional, List, Tuple
import urllib.request
import urllib.error

from core.providers.base import BaseProvider, UsageMetrics, percentage, percent_text, safe_parse
from core.logger import logger


@dataclass
class AgyProfile:
    id: str           # "default", "evanchen940628", "work"
    display_name: str # "預設 (evanchen940628@gmail.com)" or "work (evanchen940628@gmail.com)"
    short_name: str   # "預設", "evanchen", "work"
    email: Optional[str] = None
    profile_path: Optional[str] = None
    is_keyring_active: bool = False
    last_activity: Optional[float] = None


def extract_jwt_email(id_token: Optional[str]) -> Tuple[Optional[str], Optional[str]]:
    """Safe extraction of email and name from JWT payload without external libraries.
    Handles URL-safe encoding (-/_), missing padding, and malformed inputs gracefully.
    """
    if not id_token or not isinstance(id_token, str):
        return None, None
    parts = id_token.strip().split(".")
    if len(parts) < 2:
        return None, None
    payload_b64 = parts[1]
    missing_padding = len(payload_b64) % 4
    if missing_padding:
        payload_b64 += "=" * (4 - missing_padding)
    try:
        decoded = base64.urlsafe_b64decode(payload_b64.encode("ascii"))
        payload = json.loads(decoded.decode("utf-8", errors="ignore"))
        return payload.get("email"), payload.get("name")
    except Exception as e:
        logger.debug(f"[AgyProvider] Error decoding JWT: {e}")
        return None, None


def get_keyring_credential_raw() -> Optional[dict]:
    """Read raw credential dictionary from OS Keyring or fallback file."""
    # 1. Windows Credential Manager (Keyring: 'gemini:antigravity')
    if sys.platform == "win32":
        try:
            import ctypes
            from ctypes import wintypes
            class CREDENTIAL(ctypes.Structure):
                _fields_ = [
                    ('Flags', wintypes.DWORD),
                    ('Type', wintypes.DWORD),
                    ('TargetName', wintypes.LPWSTR),
                    ('Comment', wintypes.LPWSTR),
                    ('LastWritten', wintypes.FILETIME),
                    ('CredentialBlobSize', wintypes.DWORD),
                    ('CredentialBlob', ctypes.POINTER(ctypes.c_byte)),
                    ('Persist', wintypes.DWORD),
                    ('AttributeCount', wintypes.DWORD),
                    ('Attributes', ctypes.c_void_p),
                    ('TargetAlias', wintypes.LPWSTR),
                    ('UserName', wintypes.LPWSTR),
                ]
            pcred = ctypes.POINTER(CREDENTIAL)()
            advapi32 = ctypes.windll.advapi32
            if advapi32.CredReadW('gemini:antigravity', 1, 0, ctypes.byref(pcred)):
                cred = pcred.contents
                blob = ctypes.string_at(cred.CredentialBlob, cred.CredentialBlobSize)
                advapi32.CredFree(pcred)
                return json.loads(blob.decode('utf-8', errors='ignore'))
        except Exception as e:
            logger.debug(f"[AgyProvider] CredReadW failed: {e}")

    # 2. macOS Keychain ('gemini:antigravity')
    elif sys.platform == "darwin":
        try:
            out = subprocess.check_output(
                ['security', 'find-generic-password', '-s', 'gemini', '-a', 'antigravity', '-w'],
                text=True, stderr=subprocess.DEVNULL, timeout=3
            ).strip()
            return json.loads(out)
        except Exception as e:
            logger.debug(f"[AgyProvider] macOS Keychain lookup failed: {e}")

    # 3. Linux Secret Service ('gemini:antigravity')
    else:
        try:
            out = subprocess.check_output(
                ['secret-tool', 'lookup', 'service', 'gemini', 'account', 'antigravity'],
                text=True, stderr=subprocess.DEVNULL, timeout=3
            ).strip()
            return json.loads(out)
        except Exception as e:
            logger.debug(f"[AgyProvider] Linux secret-tool lookup failed: {e}")

    # 4. Local token file fallback
    token_path = os.path.expanduser("~/.gemini/antigravity-cli/antigravity-oauth-token")
    if os.path.exists(token_path):
        try:
            with open(token_path, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception as e:
            logger.debug(f"[AgyProvider] Fallback token file read failed: {e}")

    return None


def set_keyring_credential_raw(cred_data: dict) -> bool:
    """Write raw credential dictionary to OS Keyring and fallback file.
    Enforces compact JSON serialization and size guard (Windows 2560 bytes limit).
    """
    blob = json.dumps(cred_data, separators=(',', ':'), ensure_ascii=False).encode('utf-8')
    success = False

    if sys.platform == "win32":
        try:
            import ctypes
            from ctypes import wintypes
            class CREDENTIAL(ctypes.Structure):
                _fields_ = [
                    ('Flags', wintypes.DWORD),
                    ('Type', wintypes.DWORD),
                    ('TargetName', wintypes.LPWSTR),
                    ('Comment', wintypes.LPWSTR),
                    ('LastWritten', wintypes.FILETIME),
                    ('CredentialBlobSize', wintypes.DWORD),
                    ('CredentialBlob', ctypes.POINTER(ctypes.c_char)),
                    ('Persist', wintypes.DWORD),
                    ('AttributeCount', wintypes.DWORD),
                    ('Attributes', ctypes.c_void_p),
                    ('TargetAlias', wintypes.LPWSTR),
                    ('UserName', wintypes.LPWSTR),
                ]

            # Size Guard: Windows Generic Credential maximum blob size is 2560 bytes.
            # If blob exceeds 2560 bytes, id_token is stripped from keyring write to fit.
            if len(blob) > 2560 and "id_token" in cred_data:
                trimmed = dict(cred_data)
                trimmed.pop("id_token", None)
                blob = json.dumps(trimmed, separators=(',', ':')).encode('utf-8')

            cred = CREDENTIAL()
            cred.Flags = 0
            cred.Type = 1  # CRED_TYPE_GENERIC
            cred.TargetName = "gemini:antigravity"
            cred.Comment = None
            cred.CredentialBlobSize = len(blob)
            cred.CredentialBlob = ctypes.cast(ctypes.c_char_p(blob), ctypes.POINTER(ctypes.c_char))
            cred.Persist = 2  # CRED_PERSIST_LOCAL_MACHINE
            cred.AttributeCount = 0
            cred.Attributes = None
            cred.TargetAlias = None
            cred.UserName = "antigravity"

            advapi32 = ctypes.windll.advapi32
            if advapi32.CredWriteW(ctypes.byref(cred), 0):
                success = True
            else:
                logger.warning(f"[AgyProvider] CredWriteW failed, error: {ctypes.GetLastError()}")
        except Exception as e:
            logger.error(f"[AgyProvider] CredWriteW exception: {e}")

    elif sys.platform == "darwin":
        try:
            res = subprocess.run(
                ['security', 'add-generic-password', '-s', 'gemini', '-a', 'antigravity', '-w', blob.decode('utf-8'), '-U'],
                capture_output=True, timeout=5
            )
            if res.returncode == 0:
                success = True
            else:
                logger.warning(f"[AgyProvider] macOS security add-generic-password failed: {res.stderr}")
        except Exception as e:
            logger.error(f"[AgyProvider] macOS security exception: {e}")

    else:
        try:
            res = subprocess.run(
                ['secret-tool', 'store', '--label=gemini:antigravity', 'service', 'gemini', 'account', 'antigravity'],
                input=blob, capture_output=True, timeout=5
            )
            if res.returncode == 0:
                success = True
        except Exception as e:
            logger.error(f"[AgyProvider] Linux secret-tool exception: {e}")

    # Also update ~/.gemini/antigravity-cli/antigravity-oauth-token safely via atomic rename
    try:
        fb_dir = os.path.expanduser("~/.gemini/antigravity-cli")
        os.makedirs(fb_dir, exist_ok=True)
        fb_path = os.path.join(fb_dir, "antigravity-oauth-token")
        tmp = fb_path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(cred_data, f, separators=(',', ':'), ensure_ascii=False)
        os.replace(tmp, fb_path)
    except Exception as e:
        logger.debug(f"[AgyProvider] Updating fallback token file failed: {e}")

    return success


def get_profiles_dir() -> str:
    p_dir = os.path.expanduser("~/.gemini/profiles")
    os.makedirs(p_dir, exist_ok=True)
    return p_dir


def auto_capture_active_profile() -> Optional[AgyProfile]:
    """Inspect active keyring credentials and auto-capture snapshot if not already saved."""
    raw = get_keyring_credential_raw()
    if not raw or not isinstance(raw, dict):
        return None

    id_tok = raw.get("id_token")
    email, name = extract_jwt_email(id_tok)
    if not email:
        email = raw.get("email") or raw.get("user")

    if not email:
        return None

    email_clean = email.strip()
    short_name = email_clean.split("@")[0]
    p_dir = get_profiles_dir()
    os.makedirs(p_dir, exist_ok=True)
    p_path = os.path.join(p_dir, f"{short_name}.json")

    needs_save = False
    if not os.path.exists(p_path):
        needs_save = True
    else:
        try:
            with open(p_path, "r", encoding="utf-8") as f:
                saved = json.load(f)
                saved_tok = (saved.get("credential", {}).get("token", {}).get("access_token")
                             or saved.get("token", {}).get("access_token"))
                curr_tok = raw.get("token", {}).get("access_token")
                if saved_tok != curr_tok and curr_tok:
                    needs_save = True
        except Exception:
            needs_save = True

    if needs_save:
        try:
            snapshot = {
                "id": short_name,
                "email": email_clean,
                "name": name or short_name,
                "display_name": f"{short_name} ({email_clean})",
                "short_name": short_name,
                "updated_at": time.time(),
                "credential": raw
            }
            tmp = p_path + ".tmp"
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump(snapshot, f, indent=2, ensure_ascii=False)
            os.replace(tmp, p_path)
            if sys.platform != "win32":
                try:
                    os.chmod(p_path, 0o600)
                except OSError:
                    pass
        except Exception as e:
            logger.debug(f"[AgyProvider] Failed to auto-capture profile snapshot: {e}")

    return AgyProfile(
        id=short_name,
        display_name=f"{short_name} ({email_clean})",
        short_name=short_name,
        email=email_clean,
        profile_path=p_path,
        is_keyring_active=True,
        last_activity=os.path.getmtime(p_path) if os.path.exists(p_path) else time.time()
    )


def discover_profiles() -> List[AgyProfile]:
    """Auto-discover all AGY profiles:
    1. Active OS Keyring credential (auto-captured).
    2. Profiles in ~/.gemini/profiles/*.json.
    3. Sort with active profile first, followed alphabetically by ID.
    """
    profiles = []
    seen_ids = set()

    # 1. Check current keyring & auto-capture
    active_prof = auto_capture_active_profile()
    active_id = active_prof.id.lower() if active_prof else None
    if active_prof:
        seen_ids.add(active_prof.id.lower())
        profiles.append(active_prof)

    # 2. Scan ~/.gemini/profiles/*.json
    p_dir = get_profiles_dir()
    if os.path.isdir(p_dir):
        for fname in os.listdir(p_dir):
            if fname.lower().endswith(".json"):
                full_path = os.path.join(p_dir, fname)
                pid = os.path.splitext(fname)[0]
                if pid.lower() in seen_ids:
                    continue
                try:
                    with open(full_path, "r", encoding="utf-8") as f:
                        data = json.load(f)
                    email = data.get("email")
                    name = data.get("name")
                    if not email:
                        id_tok = data.get("credential", {}).get("id_token") or data.get("id_token")
                        email, name = extract_jwt_email(id_tok)
                    short = data.get("short_name") or pid
                    disp = data.get("display_name") or (f"{short} ({email})" if email else f"{short} (~/{fname})")
                    mtime = os.path.getmtime(full_path)
                    seen_ids.add(pid.lower())
                    profiles.append(AgyProfile(
                        id=pid,
                        display_name=disp,
                        short_name=short,
                        email=email,
                        profile_path=full_path,
                        is_keyring_active=(active_id is not None and pid.lower() == active_id),
                        last_activity=mtime
                    ))
                except Exception as e:
                    logger.debug(f"[AgyProvider] Error reading profile {full_path}: {e}")

    # 3. Ensure fallback default if none found
    if not profiles:
        profiles.append(AgyProfile(
            id="default",
            display_name="預設帳號 (Keyring)",
            short_name="預設",
            email=None,
            profile_path=None,
            is_keyring_active=True,
            last_activity=time.time()
        ))

    profiles.sort(key=lambda p: (0 if p.is_keyring_active else 1, p.id.lower()))
    return profiles


def resolve_active_profile(preference: str = "auto") -> Tuple[AgyProfile, bool]:
    """Resolve active profile based on preference ("auto" or profile ID)."""
    profiles = discover_profiles()
    pref = (preference or "auto").strip()

    if not pref or pref.lower() == "auto":
        for p in profiles:
            if p.is_keyring_active:
                return (p, True)
        if profiles:
            return (profiles[0], True)
        return (AgyProfile("default", "預設帳號", "預設", is_keyring_active=True), True)

    pref_lower = pref.lower()
    for p in profiles:
        if (p.id.lower() == pref_lower or
            p.short_name.lower() == pref_lower or
            (p.email and p.email.lower() == pref_lower)):
            return (p, False)

    custom_path = os.path.join(get_profiles_dir(), f"{pref}.json")
    return (AgyProfile(pref, f"{pref} (~/.gemini/profiles/{pref}.json)", pref, profile_path=custom_path), False)


def switch_active_profile(profile_id: str) -> bool:
    """Switch active profile by restoring its credentials into OS Keyring."""
    if not profile_id or profile_id == "auto":
        return True

    profiles = discover_profiles()
    target_prof = None
    for p in profiles:
        if p.id.lower() == profile_id.lower() or p.short_name.lower() == profile_id.lower():
            target_prof = p
            break

    if not target_prof or not target_prof.profile_path or not os.path.exists(target_prof.profile_path):
        logger.warning(f"[AgyProvider] Target profile {profile_id} not found or path missing")
        return False

    try:
        with open(target_prof.profile_path, "r", encoding="utf-8") as f:
            data = json.load(f)
        cred = data.get("credential") if "credential" in data else data
        if not cred or not isinstance(cred, dict):
            logger.error(f"[AgyProvider] Profile {profile_id} has invalid credential structure")
            return False

        ok = set_keyring_credential_raw(cred)
        if ok:
            logger.info(f"[AgyProvider] Successfully switched active AGY profile to {profile_id}")
        return ok
    except Exception as e:
        logger.error(f"[AgyProvider] Error switching profile to {profile_id}: {e}")
        return False


def save_current_as_profile(alias: str) -> Optional[AgyProfile]:
    """Manually snapshot current active credentials under an alias name."""
    raw = get_keyring_credential_raw()
    if not raw or not isinstance(raw, dict):
        return None
    alias_clean = alias.strip().replace(" ", "_")
    if not alias_clean:
        return None

    id_tok = raw.get("id_token")
    email, name = extract_jwt_email(id_tok)
    if not email:
        email = raw.get("email") or raw.get("user")

    p_dir = get_profiles_dir()
    os.makedirs(p_dir, exist_ok=True)
    p_path = os.path.join(p_dir, f"{alias_clean}.json")

    snapshot = {
        "id": alias_clean,
        "email": email,
        "name": name or alias_clean,
        "display_name": f"{alias_clean} ({email})" if email else alias_clean,
        "short_name": alias_clean,
        "updated_at": time.time(),
        "credential": raw
    }
    tmp = p_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(snapshot, f, indent=2, ensure_ascii=False)
    os.replace(tmp, p_path)
    if sys.platform != "win32":
        try:
            os.chmod(p_path, 0o600)
        except OSError:
            pass

    return AgyProfile(
        id=alias_clean,
        display_name=snapshot["display_name"],
        short_name=alias_clean,
        email=email,
        profile_path=p_path,
        is_keyring_active=True,
        last_activity=time.time()
    )


class AgyProvider(BaseProvider):
    provider_id = "agy"
    display_name = "AGY"
    API_URLS = (
        "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
        "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    )
    API_URL = API_URLS[0]
    HTTP_TIMEOUT = 8

    def __init__(self, timeout=30, config=None):
        self.timeout = timeout
        self.config = config
        self.profile_preference = "auto"
        self._agy_binary = None
        self._agy_binary_checked = False

    def _get_access_token(self, profile_obj: Optional[AgyProfile] = None) -> Optional[str]:
        # If a specific profile with profile_path is given, try loading from file
        if profile_obj and profile_obj.profile_path and os.path.exists(profile_obj.profile_path):
            try:
                with open(profile_obj.profile_path, "r", encoding="utf-8") as f:
                    data = json.load(f)
                cred = data.get("credential", data)
                token = cred.get("token", {}).get("access_token")
                if token:
                    return token
            except Exception as e:
                logger.debug(f"[AgyProvider] Error reading profile token from {profile_obj.profile_path}: {e}")

        # Otherwise read from system keyring
        raw = get_keyring_credential_raw()
        if raw and isinstance(raw, dict):
            token = raw.get("token", {}).get("access_token")
            if token:
                return token
        return None

    def _fetch_via_http(self, token: str, now_str: str) -> Optional[UsageMetrics]:
        headers = {
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
            "User-Agent": "antigravity/1.2.4"
        }
        for url in self.API_URLS:
            req = urllib.request.Request(url, data=b"{}", headers=headers, method="POST")
            try:
                with urllib.request.urlopen(req, timeout=self.HTTP_TIMEOUT) as resp:
                    if resp.status != 200:
                        logger.debug(f"[AgyProvider] {url} returned HTTP {resp.status}")
                        continue
                    result = self._parse_agy_json(json.loads(resp.read().decode("utf-8")), now_str)
                    if not result.error:
                        return result
                    logger.debug(f"[AgyProvider] {url} returned no usable quota data")
            except urllib.error.HTTPError as e:
                logger.debug(f"[AgyProvider] {url} returned HTTP {e.code}")
                if e.code == 401:
                    return None
            except Exception as e:
                logger.debug(f"[AgyProvider] {url} HTTP fetch error: {e}")
        return None

    def _find_agy_binary(self) -> Optional[str]:
        if self._agy_binary_checked:
            return self._agy_binary

        if sys.platform == "win32":
            local_app = os.environ.get("LOCALAPPDATA", "")
            candidates = [
                os.path.join(local_app, "agy", "bin", "agy.exe"),
                os.path.join(local_app, "agy", "bin", "agy.cmd"),
                os.path.join(local_app, "agy", "bin", "agy.bat"),
            ]
            for cand in candidates:
                if os.path.isfile(cand):
                    self._agy_binary = cand
                    break
        else:
            candidates = [
                os.path.expanduser("~/.local/bin/agy"),
                "/usr/local/bin/agy",
                os.path.expanduser("~/bin/agy")
            ]
            for cand in candidates:
                if os.path.isfile(cand):
                    self._agy_binary = cand
                    break

        if self._agy_binary is None:
            for directory in os.get_exec_path():
                names = ("agy.exe", "agy.cmd", "agy.bat") if sys.platform == "win32" else ("agy",)
                for name in names:
                    cand = os.path.join(directory, name)
                    if os.path.isfile(cand):
                        self._agy_binary = cand
                        break
                if self._agy_binary is not None:
                    break

        self._agy_binary_checked = True
        return self._agy_binary

    def fetch_usage(self) -> UsageMetrics:
        now_str = datetime.now().strftime("%H:%M:%S")
        pref = self.config.get("agy_profile", "auto") if self.config else self.profile_preference
        active_prof, is_auto = resolve_active_profile(pref)

        if is_auto:
            display_title = "ANTIGRAVITY" if active_prof.id == "default" else f"AGY [{active_prof.short_name}]"
        else:
            display_title = "ANTIGRAVITY" if active_prof.id == "default" else f"AGY ({active_prof.short_name})"

        # 1. Primary: Direct in-memory HTTP API (instant, no subprocess, zero window flash)
        token = self._get_access_token(active_prof)
        if token:
            res = self._fetch_via_http(token, now_str)
            if res and not res.error:
                res.provider_name = display_title
                return res

        # 2. Fallback: CLI execution
        metrics = self._fetch_via_cli(now_str)
        metrics.provider_name = display_title
        return metrics

    def _fetch_via_cli(self, now_str: str) -> UsageMetrics:
        agy_bin = self._find_agy_binary()
        if not agy_bin:
            logger.warning("[AgyProvider] Antigravity CLI binary not found")
            return UsageMetrics(
                provider_name="Antigravity",
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error="未找到 agy 指令\n請確認已安裝 Antigravity CLI",
                error_code="cli_not_found"
            )

        started = time.monotonic()
        try:
            kwargs = {
                "timeout": self.timeout,
                "text": True,
                "encoding": "utf-8",
                "errors": "replace",
                "capture_output": True,
                "stdin": subprocess.DEVNULL
            }
            if sys.platform == "win32":
                kwargs["creationflags"] = subprocess.CREATE_NO_WINDOW
                si = subprocess.STARTUPINFO()
                si.dwFlags |= subprocess.STARTF_USESHOWWINDOW
                si.wShowWindow = subprocess.SW_HIDE
                kwargs["startupinfo"] = si
                if agy_bin.lower().endswith((".cmd", ".bat")):
                    inner_cmd = subprocess.list2cmdline([agy_bin, "--output-format", "json", "--print", "/quota"])
                    cmd = f'cmd.exe /c "{inner_cmd}"'
                else:
                    cmd = [agy_bin, "--output-format", "json", "--print", "/quota"]
            else:
                cmd = [agy_bin, "--output-format", "json", "--print", "/quota"]

            result = subprocess.run(cmd, **kwargs)
            if result.returncode:
                return self._failure(now_str, "cli_exit", f"agy 查詢失敗 (exit {result.returncode})")

            raw = None
            out = result.stdout or ""
            out_trimmed = out.strip()
            if out_trimmed.startswith("{") and out_trimmed.endswith("}"):
                try:
                    raw = json.loads(out_trimmed)
                except Exception:
                    pass
            if raw is None:
                start_idx = out.find("{")
                end_idx = out.rfind("}")
                if start_idx != -1 and end_idx != -1 and end_idx > start_idx:
                    json_str = out[start_idx:end_idx + 1]
                    try:
                        raw = json.loads(json_str)
                    except Exception:
                        pass
            if raw is None:
                try:
                    raw = json.loads(out)
                except (ValueError, TypeError, AttributeError, KeyError, OverflowError):
                    return self._failure(now_str, "schema", "agy 配額格式不相容，請查看相容性文件")

            return self._parse_agy_json(raw, now_str)
        except subprocess.TimeoutExpired:
            return self._failure(now_str, "timeout", f"agy 配額查詢超時 ({self.timeout}s)")
        except OSError:
            return self._failure(now_str, "cli_start", "無法啟動 agy，請確認安裝與執行權限")

    def _failure(self, now_str, code, message):
        return UsageMetrics(
            provider_name="Antigravity",
            provider_id=self.provider_id,
            last_updated_time=now_str,
            error=message,
            error_code=code
        )

    @safe_parse
    def _parse_agy_json(self, raw: dict, now_str: str) -> UsageMetrics:
        if not isinstance(raw, dict):
            return self._failure(now_str, "schema", "配額格式不相容")

        groups = raw.get("command", {}).get("data", {}).get("groups", [])
        if not groups and "groups" in raw:
            groups = raw.get("groups", [])

        if not groups:
            return UsageMetrics(
                provider_name=self.display_name,
                provider_id=self.provider_id,
                metric1_val=None,
                metric1_text="--",
                metric2_val=None,
                metric2_text="--",
                last_updated_time=now_str,
                error="未取得有效配額資料",
                error_code="schema"
            )

        m1_used_pct = None
        m1_reset_dt = None
        m2_used_pct = None
        m2_reset_dt = None
        third_party_rem_pct = None

        for g in groups:
            g_name = (g.get("name") or g.get("displayName") or "").lower()
            if "gemini" in g_name:
                for b in g.get("buckets", []):
                    b_id = (b.get("id") or b.get("bucketId") or "").lower()
                    b_window = (b.get("window") or "").lower()
                    rem_frac = percentage(b.get("remaining_fraction") if "remaining_fraction" in b else b.get("remainingFraction"), 1.0)
                    if rem_frac is None:
                        continue
                    used_pct = max(0.0, min(100.0, (1.0 - rem_frac) * 100.0))

                    reset_str = b.get("reset_time") or b.get("resetTime")
                    reset_dt = None
                    if reset_str:
                        try:
                            reset_dt = datetime.fromisoformat(reset_str.replace("Z", "+00:00"))
                        except Exception:
                            pass

                    if ("5h" in b_id or "5h" in b_window) and (m1_used_pct is None or used_pct > m1_used_pct):
                        m1_used_pct = used_pct
                        m1_reset_dt = reset_dt
                    elif ("week" in b_id or "week" in b_window) and (m2_used_pct is None or used_pct > m2_used_pct):
                        m2_used_pct = used_pct
                        m2_reset_dt = reset_dt

            elif "claude" in g_name or "gpt" in g_name:
                for b in g.get("buckets", []):
                    b_id = (b.get("id") or b.get("bucketId") or "").lower()
                    if "week" in b_id:
                        rem_frac = percentage(b.get("remaining_fraction") if "remaining_fraction" in b else b.get("remainingFraction"), 1.0)
                        if rem_frac is None:
                            continue
                        remaining = rem_frac * 100.0
                        third_party_rem_pct = remaining if third_party_rem_pct is None else min(third_party_rem_pct, remaining)

        m1_text = percent_text(m1_used_pct)
        m2_text = percent_text(m2_used_pct)
        badge1 = f"C/G 剩餘: {percent_text(third_party_rem_pct)}"
        badge2 = "Gemini Models"

        return UsageMetrics(
            provider_name=self.display_name,
            provider_id=self.provider_id,
            metric1_title="SESSION 5H",
            metric1_val=m1_used_pct,
            metric1_text=m1_text,
            metric1_reset=m1_reset_dt,
            metric2_title="WEEKLY 7D",
            metric2_val=m2_used_pct,
            metric2_text=m2_text,
            metric2_reset=m2_reset_dt,
            badge1_text=badge1,
            badge2_text=badge2,
            last_updated_time=now_str,
            error="未取得有效配額資料" if m1_used_pct is None and m2_used_pct is None else None
        )
