import json
import os
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from typing import Optional

from core.providers.base import BaseProvider, UsageMetrics

class AgyProvider(BaseProvider):
    provider_id = "agy"
    display_name = "AGY"

    TOKEN_PATH = os.path.expanduser("~/.gemini/antigravity-cli/antigravity-oauth-token")
    SETTINGS_PATH = os.path.expanduser("~/.gemini/antigravity-cli/settings.json")

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

        try:
            kwargs = {
                "timeout": 8,
                "text": True,
                "encoding": "utf-8",
                "stderr": subprocess.DEVNULL
            }
            if sys.platform == "win32":
                # Avoid popping console window
                kwargs["creationflags"] = subprocess.CREATE_NO_WINDOW

            cmd = [agy_bin, "--output-format", "json", "--print", "/quota"]
            out = subprocess.check_output(cmd, **kwargs)
            raw = json.loads(out)
            return self._parse_agy_json(raw, now_str)
        except subprocess.TimeoutExpired:
            return UsageMetrics(
                provider_name="Antigravity",
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error="agy 配額查詢超時"
            )
        except Exception as e:
            return UsageMetrics(
                provider_name="Antigravity",
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error=f"配額取得失敗: {str(e)[:30]}"
            )

    def _parse_agy_json(self, raw: dict, now_str: str) -> UsageMetrics:
        groups = raw.get("command", {}).get("data", {}).get("groups", [])
        
        m1_used_pct = 0.0
        m1_reset_dt = None
        m2_used_pct = 0.0
        m2_reset_dt = None
        third_party_rem_pct = 100.0

        for g in groups:
            g_name = g.get("name", "").lower()
            if "gemini" in g_name:
                for b in g.get("buckets", []):
                    b_id = b.get("id", "").lower()
                    b_window = b.get("window", "").lower()
                    rem_frac = float(b.get("remaining_fraction", 1.0))
                    used_pct = max(0.0, min(100.0, (1.0 - rem_frac) * 100.0))
                    
                    reset_str = b.get("reset_time")
                    reset_dt = None
                    if reset_str:
                        try:
                            reset_dt = datetime.fromisoformat(reset_str.replace("Z", "+00:00"))
                        except Exception:
                            pass

                    if "5h" in b_id or "5h" in b_window:
                        m1_used_pct = used_pct
                        m1_reset_dt = reset_dt
                    elif "week" in b_id or "week" in b_window:
                        m2_used_pct = used_pct
                        m2_reset_dt = reset_dt

            elif "claude" in g_name or "gpt" in g_name:
                for b in g.get("buckets", []):
                    b_id = b.get("id", "").lower()
                    if "week" in b_id:
                        rem_frac = float(b.get("remaining_fraction", 1.0))
                        third_party_rem_pct = rem_frac * 100.0

        m1_text = f"{m1_used_pct:.1f}%" if (0 < m1_used_pct < 10) else f"{int(round(m1_used_pct))}%"
        m2_text = f"{m2_used_pct:.1f}%" if (0 < m2_used_pct < 10) else f"{int(round(m2_used_pct))}%"

        badge1 = f"Claude/GPT: {int(round(third_party_rem_pct))}%"
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
            error=None
        )

