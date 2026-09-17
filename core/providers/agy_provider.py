import json
import os
import shutil
import subprocess
import sys
import logging
import time
from datetime import datetime, timezone
from typing import Optional

from core.providers.base import BaseProvider, UsageMetrics, percentage, percent_text, safe_parse, retry_delay

class AgyProvider(BaseProvider):
    provider_id = "agy"
    display_name = "AGY"

    def __init__(self, timeout=30):
        self.timeout = timeout

    def _find_agy_binary(self) -> Optional[str]:
        # 1. PATH lookup
        p = shutil.which("agy") or shutil.which("agy.exe")
        if p and os.path.exists(p):
            return p
        
        # 2. Windows default AppData
        if sys.platform == "win32":
            local_app = os.environ.get("LOCALAPPDATA", "")
            cand = os.path.join(local_app, "agy", "bin", "agy.exe")
            if os.path.exists(cand):
                return cand
        else:
            # 3. macOS / Linux default paths
            candidates = [
                os.path.expanduser("~/.local/bin/agy"),
                "/usr/local/bin/agy",
                os.path.expanduser("~/bin/agy")
            ]
            for cand in candidates:
                if os.path.exists(cand):
                    return cand
        return None

    def fetch_usage(self) -> UsageMetrics:
        now_str = datetime.now().strftime("%H:%M:%S")
        agy_bin = self._find_agy_binary()

        if not agy_bin:
            return UsageMetrics(
                provider_name="Antigravity",
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error="未找到 agy 指令\n請確認已安裝 Antigravity CLI"
            )

        started = time.monotonic()
        try:
            kwargs = {"timeout": self.timeout, "text": True, "encoding": "utf-8",
                      "errors": "replace", "capture_output": True}
            if sys.platform == "win32":
                kwargs["creationflags"] = subprocess.CREATE_NO_WINDOW
            result = subprocess.run(
                [agy_bin, "--output-format", "json", "--print", "/quota"], **kwargs)
            # Do not persist stdout/stderr: CLI output can contain account data.
            logging.getLogger(__name__).info(
                "quota exit=%s elapsed=%.2fs stderr_chars=%s",
                result.returncode, time.monotonic() - started, len(result.stderr))
            if result.returncode:
                return self._failure(now_str, "cli_exit", f"agy 查詢失敗 (exit {result.returncode})")
            try:
                raw = json.loads(result.stdout)
                return self._parse_agy_json(raw, now_str)
            except (ValueError, TypeError, AttributeError, KeyError, OverflowError):
                return self._failure(now_str, "schema", "agy 配額格式不相容，請查看相容性文件")
        except subprocess.TimeoutExpired:
            logging.getLogger(__name__).warning("quota timeout elapsed=%.2fs", time.monotonic() - started)
            return self._failure(now_str, "timeout", f"agy 配額查詢超時 ({self.timeout}s)")
        except OSError:
            return self._failure(now_str, "cli_start", "無法啟動 agy，請確認安裝與執行權限")

    def _failure(self, now_str, code, message):
        return UsageMetrics(provider_name="Antigravity", provider_id=self.provider_id,
                            last_updated_time=now_str, error=message, error_code=code)

    @safe_parse
    def _parse_agy_json(self, raw: dict, now_str: str) -> UsageMetrics:
        groups = raw.get("command", {}).get("data", {}).get("groups", [])
        
        m1_used_pct = None
        m1_reset_dt = None
        m2_used_pct = None
        m2_reset_dt = None
        third_party_rem_pct = None

        for g in groups:
            g_name = g.get("name", "").lower()
            if "gemini" in g_name:
                for b in g.get("buckets", []):
                    b_id = b.get("id", "").lower()
                    b_window = b.get("window", "").lower()
                    rem_frac = percentage(b.get("remaining_fraction"), 1.0)
                    if rem_frac is None:
                        continue
                    used_pct = max(0.0, min(100.0, (1.0 - rem_frac) * 100.0))
                    
                    reset_str = b.get("reset_time")
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
                    b_id = b.get("id", "").lower()
                    if "week" in b_id:
                        rem_frac = percentage(b.get("remaining_fraction"), 1.0)
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

