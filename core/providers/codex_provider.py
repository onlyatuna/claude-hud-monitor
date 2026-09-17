import json
import os
import urllib.request
import urllib.error
from datetime import datetime, timezone
from typing import Optional

from core.providers.base import BaseProvider, UsageMetrics, percentage, percent_text, safe_parse, retry_delay

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
            code = "auth" if e.code == 401 else ("rate_limit" if e.code == 429 else "http")
            message = "登入憑證已失效，請使用原 CLI 重新登入" if e.code == 401 else f"配額查詢 HTTP {e.code}"
            return UsageMetrics(provider_name="OpenAI Codex", provider_id=self.provider_id,
                                last_updated_time=now_str, error=message, error_code=code,
                                retry_after=retry_delay(e.headers.get("Retry-After")) if e.headers else None)
        except (ValueError, TypeError):
            return UsageMetrics(provider_name="OpenAI Codex", provider_id=self.provider_id,
                                last_updated_time=now_str, error="未取得有效配額資料", error_code="schema")
        except Exception:
            return UsageMetrics(provider_name="OpenAI Codex", provider_id=self.provider_id,
                                last_updated_time=now_str, error="配額連線失敗，將自動重試", error_code="network")

    @safe_parse
    def _parse_response(self, data: dict, now_str: str) -> UsageMetrics:
        # OpenAI WHAM usage schema
        rl = data.get("rate_limit") or {}
        primary = rl.get("primary_window") or {}
        secondary = rl.get("secondary_window") or {}

        # 5-hour rolling usage percent
        s_used_pct = percentage(primary.get("used_percent"))
        s_reset_ts = primary.get("reset_at")
        s_reset_dt = datetime.fromtimestamp(s_reset_ts, tz=timezone.utc) if s_reset_ts else None

        # Weekly 7-day rolling usage percent
        w_used_pct = percentage(secondary.get("used_percent"))
        w_reset_ts = secondary.get("reset_at")
        w_reset_dt = datetime.fromtimestamp(w_reset_ts, tz=timezone.utc) if w_reset_ts else None

        # Quota responses do not establish which model is currently running.
        plan = data.get("plan_type")
        plan_badge = f"Plan: {plan.title()}" if isinstance(plan, str) and plan else ""

        return UsageMetrics(
            provider_name="OpenAI Codex",
            provider_id=self.provider_id,
            metric1_title=self._window_title(primary, "PRIMARY"),
            metric1_val=s_used_pct,
            metric1_text=percent_text(s_used_pct),
            metric1_reset=s_reset_dt,
            metric2_title=self._window_title(secondary, "SECONDARY"),
            metric2_val=w_used_pct,
            metric2_text=percent_text(w_used_pct),
            metric2_reset=w_reset_dt,
            badge1_text=plan_badge,
            badge2_text="",
            last_updated_time=now_str,
            error="未取得有效配額資料" if s_used_pct is None and w_used_pct is None else None
        )

    @staticmethod
    def _window_title(window, fallback):
        seconds = window.get("limit_window_seconds")
        if isinstance(seconds, (int, float)) and not isinstance(seconds, bool) and seconds > 0:
            if seconds % 86400 == 0:
                return f"WINDOW {seconds / 86400:g}D"
            if seconds % 3600 == 0:
                return f"WINDOW {seconds / 3600:g}H"
            return f"WINDOW {seconds / 60:g}M"
        return fallback
