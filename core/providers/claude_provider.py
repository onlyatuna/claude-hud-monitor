import json
import os
import urllib.request
import urllib.error
from datetime import datetime
from typing import Optional

from core.providers.base import BaseProvider, UsageMetrics

def _parse_iso_datetime(dt_val: Optional[str]) -> Optional[datetime]:
    if not dt_val:
        return None
    try:
        s = str(dt_val).strip().replace("Z", "+00:00")
        return datetime.fromisoformat(s)
    except Exception:
        return None

class ClaudeProvider(BaseProvider):
    provider_id = "claude"
    display_name = "Claude"

    CREDENTIALS_PATH = os.path.expanduser("~/.claude/.credentials.json")
    USAGE_URL = "https://api.anthropic.com/api/oauth/usage"
    USER_AGENT = "claude-code/0.2.29"
    BETA_HEADER = "oauth-2025-04-20"

    def get_access_token(self) -> Optional[str]:
        if not os.path.exists(self.CREDENTIALS_PATH):
            return None
        try:
            with open(self.CREDENTIALS_PATH, "r", encoding="utf-8") as f:
                data = json.load(f)
                return data.get("claudeAiOauth", {}).get("accessToken")
        except Exception:
            return None

    def fetch_usage(self) -> UsageMetrics:
        token = self.get_access_token()
        now_str = datetime.now().strftime("%H:%M:%S")
        if not token:
            return UsageMetrics(
                provider_name="Claude Code",
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error="未找到 Claude 登入憑證\n請於終端機執行 claude 登入"
            )

        headers = {
            "Authorization": f"Bearer {token}",
            "User-Agent": self.USER_AGENT,
            "anthropic-beta": self.BETA_HEADER,
            "Accept": "application/json"
        }

        req = urllib.request.Request(self.USAGE_URL, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=10) as resp:
                if resp.status != 200:
                    return UsageMetrics(
                        provider_name="Claude Code",
                        provider_id=self.provider_id,
                        last_updated_time=now_str,
                        error=f"API 回應異常: HTTP {resp.status}"
                    )
                raw_json = json.loads(resp.read().decode("utf-8"))
                return self._parse_response(raw_json, now_str)
        except urllib.error.HTTPError as e:
            msg = "憑證過期，請重新登入 claude" if e.code == 401 else f"連線錯誤 HTTP {e.code}"
            return UsageMetrics(provider_name="Claude Code", provider_id=self.provider_id, last_updated_time=now_str, error=msg)
        except Exception as e:
            return UsageMetrics(provider_name="Claude Code", provider_id=self.provider_id, last_updated_time=now_str, error=f"連線失敗: {str(e)[:30]}")

    def _parse_response(self, data: dict, now_str: str) -> UsageMetrics:
        five_hour = data.get("five_hour") or {}
        seven_day = data.get("seven_day") or {}
        breakdown = data.get("seven_day_breakdown") or {}
        rows = breakdown.get("rows", [])

        code_pct = 0
        chat_pct = 0
        for r in rows:
            if r.get("key") == "claude_code":
                code_pct = r.get("percent", 0)
            elif r.get("key") == "chat":
                chat_pct = r.get("percent", 0)

        five_h_dt = _parse_iso_datetime(five_hour.get("resets_at"))
        seven_d_dt = _parse_iso_datetime(seven_day.get("resets_at"))

        s_val = float(five_hour.get("utilization") or 0.0)
        w_val = float(seven_day.get("utilization") or 0.0)

        return UsageMetrics(
            provider_name="Claude Code",
            provider_id=self.provider_id,
            metric1_title="SESSION 5H",
            metric1_val=s_val,
            metric1_text=f"{s_val:.0f}%",
            metric1_reset=five_h_dt,
            metric2_title="WEEKLY 7D",
            metric2_val=w_val,
            metric2_text=f"{w_val:.0f}%",
            metric2_reset=seven_d_dt,
            badge1_text=f"Code: {code_pct}%",
            badge2_text=f"Chat: {chat_pct}%",
            last_updated_time=now_str,
            error=None
        )
