from dataclasses import dataclass
import json
import os
import subprocess
import sys
import urllib.request
import urllib.error
from datetime import datetime
from typing import Optional, List, Tuple

from core.providers.base import BaseProvider, UsageMetrics, percentage, percent_text, safe_parse, retry_delay, ssl_context
from core.logger import logger

@dataclass
class ClaudeProfile:
    id: str           # "default", "claude01", etc.
    display_name: str # "預設帳號 (~/.claude)", "claude01 (~/.claude01)"
    short_name: str   # "預設", "claude01"
    dir_path: str
    credentials_path: str
    last_activity: Optional[float] = None

def get_last_activity(dir_path: str) -> Optional[float]:
    cred = os.path.join(dir_path, ".credentials.json")
    hist = os.path.join(dir_path, "history.jsonl")
    times = []
    if os.path.exists(cred):
        try:
            times.append(os.path.getmtime(cred))
        except OSError:
            pass
    if os.path.exists(hist):
        try:
            times.append(os.path.getmtime(hist))
        except OSError:
            pass
    return max(times) if times else None

def discover_profiles() -> List[ClaudeProfile]:
    home = os.path.expanduser("~")
    profiles = []
    seen = set()

    # 1. Scan $HOME for entries starting with ".claude"
    try:
        if os.path.isdir(home):
            for entry in os.listdir(home):
                full_path = os.path.join(home, entry)
                if not os.path.isdir(full_path):
                    continue
                lower = entry.lower()
                if lower.startswith(".claude"):
                    cred_path = os.path.join(full_path, ".credentials.json")
                    is_default = (entry == ".claude")
                    if is_default or os.path.exists(cred_path):
                        norm = os.path.normpath(full_path)
                        if norm not in seen:
                            seen.add(norm)
                            if is_default:
                                pid = "default"
                                short_name = "預設"
                                display = "預設帳號 (~/.claude)"
                            else:
                                raw = entry[1:] if entry.startswith(".") else entry
                                pid = raw
                                short_name = raw
                                display = f"{raw} (~/{entry})"
                            profiles.append(ClaudeProfile(
                                id=pid,
                                display_name=display,
                                short_name=short_name,
                                dir_path=full_path,
                                credentials_path=cred_path,
                                last_activity=get_last_activity(full_path)
                            ))
    except Exception as e:
        logger.warning(f"[ClaudeProvider] Error scanning home directory for profiles: {e}")

    # 2. Check CLAUDE_CONFIG_DIR env var
    env_dir = os.environ.get("CLAUDE_CONFIG_DIR")
    if env_dir and os.path.isdir(env_dir):
        cred_path = os.path.join(env_dir, ".credentials.json")
        if os.path.exists(cred_path):
            norm = os.path.normpath(env_dir)
            if norm not in seen:
                seen.add(norm)
                base = os.path.basename(env_dir)
                raw = base[1:] if base.startswith(".") else base
                profiles.append(ClaudeProfile(
                    id=raw,
                    display_name=f"{raw} ({env_dir})",
                    short_name=raw,
                    dir_path=env_dir,
                    credentials_path=cred_path,
                    last_activity=get_last_activity(env_dir)
                ))

    # 3. Ensure default ~/.claude is included
    def_dir = os.path.join(home, ".claude")
    norm_def = os.path.normpath(def_dir)
    if norm_def not in seen:
        seen.add(norm_def)
        cred_path = os.path.join(def_dir, ".credentials.json")
        profiles.append(ClaudeProfile(
            id="default",
            display_name="預設帳號 (~/.claude)",
            short_name="預設",
            dir_path=def_dir,
            credentials_path=cred_path,
            last_activity=get_last_activity(def_dir)
        ))

    # 4. Sort: "default" first, then alphabetically
    profiles.sort(key=lambda p: (0 if p.id == "default" else 1, p.id.lower()))
    return profiles

def resolve_active_profile(preference: str = "auto") -> Tuple[ClaudeProfile, bool]:
    profiles = discover_profiles()
    pref = (preference or "auto").strip()

    if not pref or pref.lower() == "auto":
        candidates = [p for p in profiles if os.path.exists(p.credentials_path)]
        if candidates:
            best = max(candidates, key=lambda p: p.last_activity or 0)
            return (best, True)
        if profiles:
            return (profiles[0], True)
        def_dir = os.path.join(os.path.expanduser("~"), ".claude")
        return (ClaudeProfile("default", "預設帳號 (~/.claude)", "預設", def_dir, os.path.join(def_dir, ".credentials.json")), True)

    pref_lower = pref.lower()
    for p in profiles:
        if (p.id.lower() == pref_lower or 
            p.short_name.lower() == pref_lower or 
            (pref_lower.startswith(".") and p.id.lower() == pref_lower[1:]) or 
            (pref_lower == ".claude" and p.id == "default")):
            return (p, False)

    custom_dir = os.path.join(os.path.expanduser("~"), pref if pref.startswith(".") else f".{pref}")
    raw = pref[1:] if pref.startswith(".") else pref
    return (ClaudeProfile(raw, f"{raw} (~/{os.path.basename(custom_dir)})", raw, custom_dir, os.path.join(custom_dir, ".credentials.json")), False)

class ClaudeProvider(BaseProvider):
    provider_id = "claude"
    display_name = "Claude"

    DEFAULT_CREDENTIALS_PATH = os.path.expanduser("~/.claude/.credentials.json")
    CREDENTIALS_PATH = DEFAULT_CREDENTIALS_PATH
    KEYCHAIN_SERVICE = "Claude Code-credentials"
    USAGE_URL = "https://api.anthropic.com/api/oauth/usage"
    USER_AGENT = "claude-code/0.2.29"
    BETA_HEADER = "oauth-2025-04-20"

    def __init__(self, timeout=10, config=None):
        self.timeout = timeout
        self.config = config
        self.profile_preference = "auto"

    def get_access_token(self, profile: Optional[str] = None) -> Optional[str]:
        if self.CREDENTIALS_PATH != self.DEFAULT_CREDENTIALS_PATH:
            token = self._token_from_file(self.CREDENTIALS_PATH)
            if not token and sys.platform == "darwin":
                token = self._token_from_keychain()
            return token

        pref = profile or (self.config.get("claude_profile", "auto") if self.config else self.profile_preference)
        active_prof, _ = resolve_active_profile(pref)

        token = self._token_from_file(active_prof.credentials_path)
        if not token and sys.platform == "darwin" and active_prof.id == "default":
            token = self._token_from_keychain()
        return token

    def _token_from_file(self, path: Optional[str] = None) -> Optional[str]:
        target = path or self.CREDENTIALS_PATH
        if not os.path.exists(target):
            return None
        try:
            with open(target, "r", encoding="utf-8") as f:
                return self._extract_token(json.load(f))
        except Exception:
            return None

    def _token_from_keychain(self) -> Optional[str]:
        try:
            out = subprocess.run(
                ["/usr/bin/security", "find-generic-password", "-s", self.KEYCHAIN_SERVICE, "-w"],
                capture_output=True, text=True, timeout=5, stdin=subprocess.DEVNULL
            )
            if out.returncode != 0:
                return None
            return self._extract_token(json.loads(out.stdout))
        except Exception:
            return None

    @staticmethod
    def _extract_token(data) -> Optional[str]:
        if not isinstance(data, dict):
            return None
        oauth = data.get("claudeAiOauth")
        return oauth.get("accessToken") if isinstance(oauth, dict) else None

    def fetch_usage(self) -> UsageMetrics:
        pref = self.config.get("claude_profile", "auto") if self.config else self.profile_preference
        active_prof, is_auto = resolve_active_profile(pref)

        if self.CREDENTIALS_PATH != self.DEFAULT_CREDENTIALS_PATH:
            display_title = "Claude Code"
        elif is_auto:
            display_title = "CLAUDE CODE" if active_prof.id == "default" else f"CLAUDE [{active_prof.short_name}]"
        else:
            display_title = "CLAUDE CODE" if active_prof.id == "default" else f"CLAUDE ({active_prof.short_name})"

        token = self.get_access_token(pref)
        now_str = datetime.now().strftime("%H:%M:%S")
        if not token:
            err_msg = ("未找到 Claude 登入憑證\n請於終端機執行 claude 登入" if active_prof.id == "default"
                       else f"未找到帳號 [{active_prof.short_name}] 登入憑證\n請使用 {active_prof.short_name} 登入或檢查目錄")
            return UsageMetrics(
                provider_name=display_title,
                provider_id=self.provider_id,
                last_updated_time=now_str,
                error=err_msg
            )

        headers = {
            "Authorization": f"Bearer {token}",
            "User-Agent": self.USER_AGENT,
            "anthropic-beta": self.BETA_HEADER,
            "Accept": "application/json"
        }

        req = urllib.request.Request(self.USAGE_URL, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=self.timeout, context=ssl_context()) as resp:
                if resp.status != 200:
                    return UsageMetrics(
                        provider_name=display_title,
                        provider_id=self.provider_id,
                        last_updated_time=now_str,
                        error=f"API 回應異常: HTTP {resp.status}"
                    )
                raw_json = json.loads(resp.read().decode("utf-8"))
                metrics = self._parse_response(raw_json, now_str)
                metrics.provider_name = display_title
                return metrics
        except urllib.error.HTTPError as e:
            logger.warning(f"[ClaudeProvider] HTTP error: {e.code}")
            code = "auth" if e.code == 401 else ("rate_limit" if e.code == 429 else "http")
            message = "登入憑證已失效，請使用原 CLI 重新登入" if e.code == 401 else f"配額查詢 HTTP {e.code}"
            return UsageMetrics(provider_name=display_title, provider_id=self.provider_id,
                                last_updated_time=now_str, error=message, error_code=code,
                                retry_after=retry_delay(e.headers.get("Retry-After")) if e.headers else None)
        except (ValueError, TypeError):
            return UsageMetrics(provider_name=display_title, provider_id=self.provider_id,
                                last_updated_time=now_str, error="未取得有效配額資料", error_code="schema")
        except Exception as e:
            logger.error(f"[ClaudeProvider] Error fetching usage: {e}", exc_info=True)
            return UsageMetrics(provider_name=display_title, provider_id=self.provider_id,
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
