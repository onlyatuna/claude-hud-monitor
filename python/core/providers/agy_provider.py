import json
import os
import subprocess
import sys
import logging
import time
from datetime import datetime, timezone
from typing import Optional
import urllib.request
import urllib.error

from core.providers.base import BaseProvider, UsageMetrics, percentage, percent_text, safe_parse
from core.logger import logger

class AgyProvider(BaseProvider):
    provider_id = "agy"
    display_name = "AGY"
    API_URLS = (
        "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
        "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    )
    API_URL = API_URLS[0]
    HTTP_TIMEOUT = 8

    def __init__(self, timeout=30):
        self.timeout = timeout
        self._agy_binary = None
        self._agy_binary_checked = False

    def _get_access_token(self) -> Optional[str]:
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
                    data = json.loads(blob.decode('utf-8', errors='ignore'))
                    token = data.get('token', {}).get('access_token')
                    if token:
                        return token
            except Exception as e:
                logger.debug(f"[AgyProvider] CredReadW failed: {e}")

        # 2. macOS Keychain ('gemini:antigravity')
        elif sys.platform == "darwin":
            try:
                out = subprocess.check_output(
                    ['security', 'find-generic-password', '-s', 'gemini', '-a', 'antigravity', '-w'],
                    text=True, stderr=subprocess.DEVNULL, timeout=3
                ).strip()
                data = json.loads(out)
                token = data.get('token', {}).get('access_token')
                if token:
                    return token
            except Exception as e:
                logger.debug(f"[AgyProvider] macOS Keychain lookup failed: {e}")

        # 3. Local token file fallback
        token_path = os.path.expanduser("~/.gemini/antigravity-cli/antigravity-oauth-token")
        if os.path.exists(token_path):
            try:
                with open(token_path, "r", encoding="utf-8") as f:
                    data = json.load(f)
                    token = data.get("token", {}).get("access_token")
                    if token:
                        return token
            except Exception as e:
                logger.debug(f"[AgyProvider] Token file read failed: {e}")

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

        # 1. Primary: Direct in-memory HTTP API (instant, no subprocess, zero window flash)
        token = self._get_access_token()
        if token:
            res = self._fetch_via_http(token, now_str)
            if res and not res.error:
                return res

        # 2. Fallback: CLI execution
        return self._fetch_via_cli(now_str)

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
                provider_name="Antigravity",
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
            provider_name="Antigravity",
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
