import json
import os
import urllib.request
import urllib.error
from datetime import datetime, timezone
from typing import Optional

from core.providers.base import BaseProvider, UsageMetrics

class CodexProvider(BaseProvider):
    provider_id = "codex"
    display_name = "Codex"

    AUTH_PATH = os.path.expanduser("~/.codex/auth.json")
    USAGE_URL = "https://chatgpt.com/backend-api/wham/usage"

    def get_auth_data(self) -> Optional[dict]:
        if not os.path.exists(self.AUTH_PATH):
            return None
        try:
            with open(self.AUTH_PATH, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            return None

    def fetch_usage(self) -> UsageMetrics:
        now_str = datetime.now().strftime("%H:%M:%S")
        auth_data = self.get_auth_data()

        if not auth_data:
            return UsageMetrics(
                provider_name="OpenAI Codex",
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error="未找到 Codex 授權檔 (~/.codex/auth.json)\n請執行 codex 登入"
            )

        tokens = auth_data.get("tokens") or {}
        access_token = tokens.get("access_token")
        account_id = tokens.get("account_id")

        if not access_token:
            return UsageMetrics(
                provider_name="OpenAI Codex",
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error="未找到 access_token\n請於終端機執行 codex 登入"
            )

        headers = {
            "Authorization": f"Bearer {access_token}",
            "User-Agent": "codex-cli/0.154.0",
            "Accept": "application/json"
        }
        if account_id:
            headers["ChatGPT-Account-Id"] = account_id

        req = urllib.request.Request(self.USAGE_URL, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=8) as resp:
                raw_json = json.loads(resp.read().decode("utf-8"))
                return self._parse_response(raw_json, now_str)
        except urllib.error.HTTPError as e:
            if e.code == 401:
                return UsageMetrics(
                    provider_name="OpenAI Codex",
                    provider_id=self.provider_id,
                    last_updated_time=now_str,
                    error="Codex 憑證過期，請於終端機執行 codex 重新授權"
                )
            return UsageMetrics(
                provider_name="OpenAI Codex",
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error=f"API 錯誤: HTTP {e.code}"
            )
        except Exception as e:
            return UsageMetrics(
                provider_name="OpenAI Codex",
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error=f"連線失敗: {str(e)[:30]}"
            )

    def _parse_response(self, data: dict, now_str: str) -> UsageMetrics:
        # OpenAI WHAM usage schema
        rl = data.get("rate_limit") or {}
        primary = rl.get("primary_window") or {}
        secondary = rl.get("secondary_window") or {}

        # 5-hour rolling usage percent
        s_used_pct = float(primary.get("used_percent") or 0.0)
        s_reset_ts = primary.get("reset_at")
        s_reset_dt = datetime.fromtimestamp(s_reset_ts, tz=timezone.utc) if s_reset_ts else None

        # Weekly 7-day rolling usage percent
        w_used_pct = float(secondary.get("used_percent") or 0.0)
        w_reset_ts = secondary.get("reset_at")
        w_reset_dt = datetime.fromtimestamp(w_reset_ts, tz=timezone.utc) if w_reset_ts else None

        # Model and plan
        plan = data.get("plan_type", "Plus").title()
        model_usage = data.get("model_usage") or {}
        models = list(model_usage.keys())
        model_name = models[0] if models else "gpt-6-astra"

        return UsageMetrics(
            provider_name="OpenAI Codex",
            provider_id=self.provider_id,
            metric1_title="SESSION 5H",
            metric1_val=s_used_pct,
            metric1_text=f"{s_used_pct:.0f}%",
            metric1_reset=s_reset_dt,
            metric2_title="WEEKLY 7D",
            metric2_val=w_used_pct,
            metric2_text=f"{w_used_pct:.0f}%",
            metric2_reset=w_reset_dt,
            badge1_text=f"{model_name}",
            badge2_text=f"Plan: {plan}",
            last_updated_time=now_str,
            error=None
        )
