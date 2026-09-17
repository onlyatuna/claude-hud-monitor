import json
import os
import urllib.request
import urllib.error
from datetime import datetime
from typing import Optional

from core.providers.base import BaseProvider, UsageMetrics, percentage, percent_text, safe_parse, retry_delay

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
            code = "auth" if e.code == 401 else ("rate_limit" if e.code == 429 else "http")
            message = "登入憑證已失效，請使用原 CLI 重新登入" if e.code == 401 else f"配額查詢 HTTP {e.code}"
            return UsageMetrics(provider_name="Claude Code", provider_id=self.provider_id,
                                last_updated_time=now_str, error=message, error_code=code,
                                retry_after=retry_delay(e.headers.get("Retry-After")) if e.headers else None)
        except (ValueError, TypeError):
            return UsageMetrics(provider_name="Claude Code", provider_id=self.provider_id,
                                last_updated_time=now_str, error="未取得有效配額資料", error_code="schema")
        except Exception:
            return UsageMetrics(provider_name="Claude Code", provider_id=self.provider_id,
                                last_updated_time=now_str, error="配額連線失敗，將自動重試", error_code="network")

    @safe_parse
    def _parse_response(self, data: dict, now_str: str) -> UsageMetrics:
        five_hour = data.get("five_hour") or {}
        seven_day = data.get("seven_day") or {}
        breakdown = data.get("seven_day_breakdown") or {}
        rows = breakdown.get("rows", [])

        code_pct = None
        chat_pct = None
        for r in rows:
            if r.get("key") == "claude_code":
                code_pct = percentage(r.get("percent"))
            elif r.get("key") == "chat":
                chat_pct = percentage(r.get("percent"))

        five_h_dt = None
        if five_hour.get("resets_at"):
            try:
                five_h_dt = datetime.fromisoformat(five_hour["resets_at"])
            except Exception:
                pass

        seven_d_dt = None
        if seven_day.get("resets_at"):
            try:
                seven_d_dt = datetime.fromisoformat(seven_day["resets_at"])
            except Exception:
                pass

        s_val = percentage(five_hour.get("utilization"))
        w_val = percentage(seven_day.get("utilization"))

        return UsageMetrics(
            provider_name="Claude Code",
            provider_id=self.provider_id,
            metric1_title="SESSION 5H",
            metric1_val=s_val,
            metric1_text=percent_text(s_val),
            metric1_reset=five_h_dt,
            metric2_title="WEEKLY 7D",
            metric2_val=w_val,
            metric2_text=percent_text(w_val),
            metric2_reset=seven_d_dt,
            badge1_text=f"Code: {percent_text(code_pct)}",
            badge2_text=f"Chat: {percent_text(chat_pct)}",
            last_updated_time=now_str,
            error="未取得有效配額資料" if s_val is None and w_val is None else None
        )
