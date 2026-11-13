import json
import os
import urllib.request
import urllib.error
from datetime import datetime, timezone
from typing import Optional

from core.providers.base import BaseProvider, UsageMetrics, percentage, percent_text, safe_parse, retry_delay, ssl_context
from core.logger import logger

def _parse_timestamp(ts) -> Optional[datetime]:
    if ts is None:
        return None
    try:
        val = float(ts)
        if val > 1e11:
            val /= 1000.0
        return datetime.fromtimestamp(val, tz=timezone.utc)
    except Exception as e:
        logger.warning(f"[CodexProvider] Timestamp parse error for '{ts}': {e}")
        return None

class CodexProvider(BaseProvider):
    provider_id = "codex"
    display_name = "Codex"

    AUTH_PATH = os.path.expanduser("~/.codex/auth.json")
    USAGE_URL = "https://chatgpt.com/backend-api/wham/usage"

    def __init__(self, timeout=8):
        self.timeout = timeout

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
                error="未找到 Codex 授權檔 (~/.codex/auth.json)\n請執行 codex 登入",
                error_code="auth"
            )

        tokens = auth_data.get("tokens") or {}
        access_token = tokens.get("access_token")
        account_id = tokens.get("account_id")

        if not access_token:
            return UsageMetrics(
                provider_name="OpenAI Codex",
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error="未找到 access_token\n請於終端機執行 codex 登入",
                error_code="auth"
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
            with urllib.request.urlopen(req, timeout=self.timeout, context=ssl_context()) as resp:
                raw_json = json.loads(resp.read().decode("utf-8"))
                return self._parse_response(raw_json, now_str)
        except urllib.error.HTTPError as e:
            logger.warning(f"[CodexProvider] HTTP error: {e.code}")
            code = "auth" if e.code in (401, 403) else ("rate_limit" if e.code == 429 else "http")
            message = "登入憑證已失效，請使用原 CLI 重新登入" if code == "auth" else f"配額查詢 HTTP {e.code}"
            return UsageMetrics(provider_name="OpenAI Codex", provider_id=self.provider_id,
                                last_updated_time=now_str, error=message, error_code=code,
                                retry_after=retry_delay(e.headers.get("Retry-After")) if e.headers else None)
        except (ValueError, TypeError):
            return UsageMetrics(provider_name="OpenAI Codex", provider_id=self.provider_id,
                                last_updated_time=now_str, error="未取得有效配額資料", error_code="schema")
        except Exception as e:
            logger.error(f"[CodexProvider] Error fetching usage: {e}", exc_info=True)
            return UsageMetrics(provider_name="OpenAI Codex", provider_id=self.provider_id,
                                last_updated_time=now_str, error="配額連線失敗，將自動重試", error_code="network")

    @safe_parse
    def _parse_response(self, data: dict, now_str: str) -> UsageMetrics:
        # OpenAI WHAM usage schema
        rl = data.get("rate_limit") or {}
        # Windows are identified by their length, so a plan with a single window (e.g. one 30-day window and a
        # null secondary_window) fills the first row and leaves the second one empty instead of faking it.
        windows = [w for w in (rl.get("primary_window"), rl.get("secondary_window")) if isinstance(w, dict) and w]
        windows += [{}] * (2 - len(windows))
        first, second = windows[0], windows[1]

        first_pct = percentage(first.get("used_percent"))
        second_pct = percentage(second.get("used_percent"))

        # Quota responses do not establish which model is currently running.
        plan = data.get("plan_type")
        plan_badge = f"Plan: {plan.title()}" if isinstance(plan, str) and plan else ""

        return UsageMetrics(
            provider_name="OpenAI Codex",
            provider_id=self.provider_id,
            metric1_title=self._window_title(first, "PRIMARY"),
            metric1_val=first_pct,
            metric1_text=percent_text(first_pct),
            metric1_reset=_parse_timestamp(first.get("reset_at")),
            metric2_title=self._window_title(second, "SECONDARY" if second else ""),
            metric2_val=second_pct,
            metric2_text=percent_text(second_pct),
            metric2_reset=_parse_timestamp(second.get("reset_at")),
            badge1_text=plan_badge,
            badge2_text="",
            last_updated_time=now_str,
            error="未取得有效配額資料" if first_pct is None and second_pct is None else None
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
