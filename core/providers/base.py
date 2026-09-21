from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Optional
import math
import ssl
from functools import lru_cache, wraps
from email.utils import parsedate_to_datetime


def percentage(value, maximum=100.0):
    """Return None for unavailable or invalid quota, preserving genuine zero."""
    if value is None or isinstance(value, bool):
        return None
    try:
        result = float(value)
    except (TypeError, ValueError):
        return None
    return result if math.isfinite(result) and 0 <= result <= maximum else None


def percent_text(value):
    return "--" if value is None else f"{value:.0f}%"


def safe_parse(method):
    @wraps(method)
    def wrapped(self, data, now_str):
        try:
            if not isinstance(data, dict):
                raise ValueError("expected object")
            result = method(self, data, now_str)
            if result.error and not result.error_code:
                result.error_code = "schema"
            return result
        except (ValueError, TypeError, AttributeError, KeyError, OverflowError, OSError):
            return UsageMetrics(provider_id=self.provider_id, provider_name=self.display_name,
                                last_updated_time=now_str, error="配額資料格式不相容", error_code="schema")
    return wrapped


@lru_cache(maxsize=1)
def ssl_context():
    """Verify HTTPS against certifi's CA bundle when available.

    Frozen builds made with python.org Python cannot see the macOS system
    trust store, so the default context fails with CERTIFICATE_VERIFY_FAILED.
    """
    try:
        import certifi
        return ssl.create_default_context(cafile=certifi.where())
    except (ImportError, OSError):
        return ssl.create_default_context()


def retry_delay(value):
    """Accept Retry-After seconds or an HTTP date, never persist header contents."""
    if not value:
        return None
    seconds = percentage(value, float("inf"))
    if seconds is not None:
        return seconds
    try:
        target = parsedate_to_datetime(value)
        return max(0, (target - datetime.now(timezone.utc)).total_seconds())
    except (ValueError, TypeError, OverflowError):
        return None


@dataclass
class UsageMetrics:
    provider_name: str = ""
    provider_id: str = ""
    
    metric1_title: str = "SESSION 5H"
    metric1_val: Optional[float] = None
    metric1_text: str = "--"
    metric1_reset: Optional[datetime] = None
    metric1_subtext: str = ""

    metric2_title: str = "WEEKLY 7D"
    metric2_val: Optional[float] = None
    metric2_text: str = "--"
    metric2_reset: Optional[datetime] = None
    metric2_subtext: str = ""

    badge1_text: str = ""
    badge2_text: str = ""
    
    last_updated_time: str = ""
    error: Optional[str] = None
    error_code: str = ""
    retry_after: Optional[float] = None
    stale: bool = False
    last_success: Optional[datetime] = None

class BaseProvider:
    provider_id: str = "base"
    display_name: str = "Base"

    def fetch_usage(self) -> UsageMetrics:
        raise NotImplementedError

    @staticmethod
    def format_countdown(target_dt: Optional[datetime]) -> str:
        if not target_dt:
            return "--"
        if target_dt.tzinfo is None:
            now = datetime.now()
        else:
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
