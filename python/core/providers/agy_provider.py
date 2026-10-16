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

from core.providers.base import BaseProvider, UsageMetrics, MetricRow, percentage, percent_text, safe_parse
from core.logger import logger
from core.agy_statusline import read_quota_cache

class AgyProvider(BaseProvider):
    provider_id = "agy"
    display_name = "AGY"
    SUMMARY_URL = "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary"
    QUOTA_URL = "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota"
    HTTP_TIMEOUT = 8

    def __init__(self, timeout=30):
        self.timeout = timeout

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

    def _fetch_via_http(self, token: str, now_str: str) -> UsageMetrics:
        headers = {
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
            "User-Agent": "antigravity/1.2.4"
        }

        summary_metrics: Optional[UsageMetrics] = None
        req_a = urllib.request.Request(self.SUMMARY_URL, data=b"{}", headers=headers, method="POST")
        try:
            with urllib.request.urlopen(req_a, timeout=self.HTTP_TIMEOUT) as resp:
                if resp.status == 401:
                    return self._failure(now_str, "auth", "登入憑證已失效，請使用原 CLI 重新登入")
                if resp.status == 200:
                    parsed = self._parse_agy_json(json.loads(resp.read().decode("utf-8")), now_str)
                    if not parsed.error:
                        summary_metrics = parsed
                    else:
                        logger.debug(f"[AgyProvider] {self.SUMMARY_URL} returned no usable quota data")
                else:
                    logger.debug(f"[AgyProvider] {self.SUMMARY_URL} returned HTTP {resp.status}")
        except urllib.error.HTTPError as e:
            logger.debug(f"[AgyProvider] {self.SUMMARY_URL} returned HTTP {e.code}")
            if e.code == 401:
                return self._failure(now_str, "auth", "登入憑證已失效，請使用原 CLI 重新登入")
        except Exception as e:
            logger.debug(f"[AgyProvider] {self.SUMMARY_URL} HTTP fetch error: {e}")

        quota_metrics: Optional[UsageMetrics] = None
        req_b = urllib.request.Request(self.QUOTA_URL, data=b"{}", headers=headers, method="POST")
        try:
            with urllib.request.urlopen(req_b, timeout=self.HTTP_TIMEOUT) as resp:
                if resp.status == 401:
                    return self._failure(now_str, "auth", "登入憑證已失效，請使用原 CLI 重新登入")
                if resp.status == 200:
                    parsed = self._parse_model_buckets(json.loads(resp.read().decode("utf-8")), now_str)
                    if not parsed.error:
                        quota_metrics = parsed
                    else:
                        logger.debug(f"[AgyProvider] {self.QUOTA_URL} returned no usable quota data")
                else:
                    logger.debug(f"[AgyProvider] {self.QUOTA_URL} returned HTTP {resp.status}")
        except urllib.error.HTTPError as e:
            logger.debug(f"[AgyProvider] {self.QUOTA_URL} returned HTTP {e.code}")
            if e.code == 401:
                return self._failure(now_str, "auth", "登入憑證已失效，請使用原 CLI 重新登入")
        except Exception as e:
            logger.debug(f"[AgyProvider] {self.QUOTA_URL} HTTP fetch error: {e}")

        m1_val = quota_metrics.metric1_val if quota_metrics else None
        m1_reset = quota_metrics.metric1_reset if quota_metrics else None
        badge1 = summary_metrics.badge1_text if summary_metrics and summary_metrics.badge1_text else f"C/G 剩餘: {percent_text(None)}"
        rows = summary_metrics.extra_metrics if summary_metrics else self._empty_rows()
        has_cg = any(row.val is not None for row in rows[:2])
        has_error = m1_val is None and not has_cg

        return UsageMetrics(
            provider_name="Antigravity",
            provider_id=self.provider_id,
            metric1_title="GEMINI 5H",
            metric1_val=m1_val,
            metric1_text=percent_text(m1_val),
            metric1_reset=m1_reset,
            metric2_title="GEMINI 7D",
            metric2_val=None,
            metric2_text="--",
            metric2_reset=None,
            metric2_subtext="Gemini 週額度來源未知",
            badge1_text=badge1,
            badge2_text="Gemini Models",
            last_updated_time=now_str,
            error="未取得有效配額資料" if has_error else None,
            error_code="schema" if has_error else "",
            extra_metrics=[] if has_error else rows,
        )

    def fetch_usage(self) -> UsageMetrics:
        now_str = datetime.now().strftime("%H:%M:%S")
        try:
            cache = read_quota_cache()
        except Exception:
            cache = None
        token = self._get_access_token()
        res = self._fetch_via_http(token, now_str) if token else None
        if cache is None:
            if res is None:
                res = self._failure(now_str, "no_token", "未找到 Antigravity 登入憑證\n請先登入 Antigravity")
            res.badge2_text = "Argon: --"
            return res
        return self._merge_cache(res, cache, now_str)

    def _merge_cache(self, res, cache, now_str) -> UsageMetrics:
        result = res if res is not None else UsageMetrics()
        buckets = cache.get("buckets", {}) if cache is not None else {}
        rows = result.extra_metrics or self._empty_rows()
        for row, key in zip(rows, ("3p-5h", "3p-weekly", "gemini-argon-wtu-weekly")):
            bucket = buckets.get(key)
            if row.val is None and bucket:
                row.val = max(0.0, min(100.0, (1 - bucket["remaining_fraction"]) * 100))
                row.text = percent_text(row.val)
                row.reset = bucket["reset_time"]
                row.subtext = ""
        result.extra_metrics = rows
        if result.metric1_val is None and "gemini-5h" in buckets:
            bucket = buckets["gemini-5h"]
            result.metric1_val = max(0.0, min(100.0, (1 - bucket["remaining_fraction"]) * 100))
            result.metric1_reset = bucket["reset_time"]
        weekly = buckets.get("gemini-weekly")
        result.metric2_val = max(0.0, min(100.0, (1 - weekly["remaining_fraction"]) * 100)) if weekly else None
        result.metric2_reset = weekly["reset_time"] if weekly else None
        result.metric2_subtext = "" if weekly else "需啟用 agy statusLine"
        if res is None or result.badge1_text in ("", "C/G 剩餘: --"):
            cg = buckets.get("3p-weekly")
            result.badge1_text = f"C/G 剩餘: {percent_text(cg['remaining_fraction'] * 100 if cg else None)}"
        argon = buckets.get("gemini-argon-wtu-weekly")
        result.badge2_text = f"Argon 剩餘: {percent_text(argon['remaining_fraction'] * 100 if argon else None)}"
        result.metric1_title = "GEMINI 5H"
        result.metric2_title = "GEMINI 7D"
        result.metric1_text = percent_text(result.metric1_val)
        result.metric2_text = percent_text(result.metric2_val)
        result.provider_name = "Antigravity"
        result.provider_id = self.provider_id
        result.last_updated_time = now_str
        has_value = (result.metric1_val is not None or result.metric2_val is not None
                     or any(row.val is not None for row in rows))
        result.error = None if has_value else "未取得有效配額資料"
        result.error_code = "" if has_value else "schema"
        if not has_value:
            result.extra_metrics = []
        return result

    @staticmethod
    def _empty_rows():
        return [MetricRow("C/G 5H"), MetricRow("C/G 7D"),
                MetricRow("ARGON 7D", subtext="需啟用 agy statusLine")]

    def _failure(self, now_str, code, message):
        return UsageMetrics(
            provider_name="Antigravity",
            provider_id=self.provider_id,
            last_updated_time=now_str,
            error=message,
            error_code=code
        )

    @safe_parse
    def _parse_model_buckets(self, raw: dict, now_str: str) -> UsageMetrics:
        if not isinstance(raw, dict):
            return self._failure(now_str, "schema", "配額格式不相容")

        buckets = raw.get("buckets", [])
        if not isinstance(buckets, list):
            raise TypeError("expected list")

        m1_used_pct = None
        m1_reset_dt = None

        for b in buckets:
            model_id = (b.get("modelId") or b.get("model_id") or "").lower()
            if "gemini" not in model_id:
                continue
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

            if m1_used_pct is None or used_pct > m1_used_pct:
                m1_used_pct = used_pct
                m1_reset_dt = reset_dt

        return UsageMetrics(
            provider_name="Antigravity",
            provider_id=self.provider_id,
            metric1_title="GEMINI 5H",
            metric1_val=m1_used_pct,
            metric1_text=percent_text(m1_used_pct),
            metric1_reset=m1_reset_dt,
            metric2_title="GEMINI 7D",
            metric2_val=None,
            metric2_text="--",
            metric2_reset=None,
            metric2_subtext="Gemini 週額度來源未知",
            badge2_text="Gemini Models",
            last_updated_time=now_str,
            error="未取得有效配額資料" if m1_used_pct is None else None,
            error_code="schema" if m1_used_pct is None else "",
            extra_metrics=[] if m1_used_pct is None else self._empty_rows(),
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
                metric2_subtext="Gemini 週額度來源未知",
                last_updated_time=now_str,
                error="未取得有效配額資料",
                error_code="schema"
            )

        m1_used_pct = None
        m1_reset_dt = None
        m2_used_pct = None
        m2_reset_dt = None
        third_party_rem_pct = None
        rows = self._empty_rows()

        for g in groups:
            name = g.get("name") or g.get("displayName") or ""
            g_name = name.lower()
            if "gemini" in g_name and "argon" not in g_name:
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

            elif ("claude" in g_name or "gpt" in g_name) and "argon" not in g_name:
                for b in g.get("buckets", []):
                    b_id = (b.get("id") or b.get("bucketId") or "").lower()
                    row = rows[1] if "week" in b_id else rows[0] if "5h" in b_id else None
                    if row is None:
                        continue
                    rem_frac = percentage(b.get("remaining_fraction") if "remaining_fraction" in b else b.get("remainingFraction"), 1.0)
                    if rem_frac is None:
                        continue
                    used = (1 - rem_frac) * 100
                    if row.val is None or used > row.val:
                        row.val = used
                        row.text = percent_text(used)
                        reset = b.get("reset_time") or b.get("resetTime")
                        try:
                            row.reset = datetime.fromisoformat(reset.replace("Z", "+00:00")) if reset else None
                        except (ValueError, TypeError, AttributeError):
                            row.reset = None
                    if row is rows[1]:
                        third_party_rem_pct = 100 - row.val
            else:
                logger.info(f"[AgyProvider] unknown quota group: {name}")

        m1_text = percent_text(m1_used_pct)
        m2_text = percent_text(m2_used_pct)
        badge1 = f"C/G 剩餘: {percent_text(third_party_rem_pct)}"
        badge2 = "Gemini Models"
        has_error = m1_used_pct is None and m2_used_pct is None and all(row.val is None for row in rows)

        return UsageMetrics(
            provider_name="Antigravity",
            provider_id=self.provider_id,
            metric1_title="GEMINI 5H",
            metric1_val=m1_used_pct,
            metric1_text=m1_text,
            metric1_reset=m1_reset_dt,
            metric2_title="GEMINI 7D",
            metric2_val=m2_used_pct,
            metric2_text=m2_text,
            metric2_reset=m2_reset_dt,
            metric2_subtext="Gemini 週額度來源未知" if m2_used_pct is None else "",
            badge1_text=badge1,
            badge2_text=badge2,
            last_updated_time=now_str,
            error="未取得有效配額資料" if has_error else None,
            error_code="schema" if has_error else "",
            extra_metrics=[] if has_error else rows,
        )
