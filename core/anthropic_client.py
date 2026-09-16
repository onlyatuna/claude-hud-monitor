import json
import os
import urllib.request
import urllib.error
from datetime import datetime, timezone
from dataclasses import dataclass
from typing import Optional

@dataclass
class UsageData:
    session_utilization: float = 0.0
    session_resets_at: Optional[datetime] = None
    weekly_utilization: float = 0.0
    weekly_resets_at: Optional[datetime] = None
    code_percent: int = 0
    chat_percent: int = 0
    last_updated_time: str = ""
    error: Optional[str] = None

class AnthropicClient:
    CREDENTIALS_PATH = os.path.expanduser("~/.claude/.credentials.json")
    USAGE_URL = "https://api.anthropic.com/api/oauth/usage"
    USER_AGENT = "claude-code/0.2.29"
    BETA_HEADER = "oauth-2025-04-20"

    def __init__(self):
        self._cached_data: Optional[UsageData] = None

    def get_access_token(self) -> Optional[str]:
        if not os.path.exists(self.CREDENTIALS_PATH):
            return None
        try:
            with open(self.CREDENTIALS_PATH, "r", encoding="utf-8") as f:
                data = json.load(f)
                return data.get("claudeAiOauth", {}).get("accessToken")
        except Exception as e:
            print(f"[Client] Failed reading credentials: {e}")
            return None

    def fetch_usage(self) -> UsageData:
        token = self.get_access_token()
        if not token:
            return UsageData(error="未找到 Claude Code 登入憑證\n請先執行 claude 登入")

        headers = {
            "Authorization": f"Bearer {token}",
            "User-Agent": self.USER_AGENT,
            "anthropic-beta": self.BETA_HEADER,
            "Accept": "application/json"
        }

        req = urllib.request.Request(self.USAGE_URL, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=10) as resp:
                status = resp.status
                if status != 200:
                    return UsageData(error=f"API 回應異常: HTTP {status}")
                raw_body = resp.read().decode("utf-8")
                raw_json = json.loads(raw_body)
                return self._parse_response(raw_json)
        except urllib.error.HTTPError as e:
            if e.code == 401:
                return UsageData(error="憑證已過期，請於終端重新登入 claude")
            elif e.code == 429:
                return UsageData(error="請求過於頻繁 (HTTP 429)，冷卻中")
            return UsageData(error=f"連線錯誤 HTTP {e.code}")
        except urllib.error.URLError as e:
            return UsageData(error=f"網路連線失敗: {e.reason}")
        except Exception as e:
            return UsageData(error=f"未知錯誤: {str(e)}")

    def _parse_response(self, data: dict) -> UsageData:
        five_hour = data.get("five_hour") or {}
        seven_day = data.get("seven_day") or {}
        seven_day_breakdown = data.get("seven_day_breakdown") or {}
        rows = seven_day_breakdown.get("rows", [])

        code_pct = 0
        chat_pct = 0
        for r in rows:
            if r.get("key") == "claude_code":
                code_pct = r.get("percent", 0)
            elif r.get("key") == "chat":
                chat_pct = r.get("percent", 0)

        # Parse resets_at ISO datetime
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

        now_str = datetime.now().strftime("%H:%M:%S")

        usage = UsageData(
            session_utilization=float(five_hour.get("utilization") or 0.0),
            session_resets_at=five_h_dt,
            weekly_utilization=float(seven_day.get("utilization") or 0.0),
            weekly_resets_at=seven_d_dt,
            code_percent=code_pct,
            chat_percent=chat_pct,
            last_updated_time=now_str,
            error=None
        )
        self._cached_data = usage
        return usage

    @staticmethod
    def format_countdown(target_dt: Optional[datetime]) -> str:
        if not target_dt:
            return "--"
        now = datetime.now(timezone.utc)
        diff = target_dt - now
        total_sec = int(diff.total_seconds())
        if total_sec <= 0:
            return "即將重設"

        days = total_sec // 86400
        hours = (total_sec % 86400) // 3600
        mins = (total_sec % 3600) // 60
        secs = total_sec % 60

        if days > 0:
            return f"{days}天 {hours}時 {mins}分"
        elif hours > 0:
            return f"{hours}h {mins:02d}m {secs:02d}s"
        else:
            return f"{mins}m {secs:02d}s"
