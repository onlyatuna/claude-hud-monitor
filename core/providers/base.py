from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Optional

@dataclass
class UsageMetrics:
    provider_name: str = ""
    provider_id: str = ""
    
    metric1_title: str = "SESSION 5H"
    metric1_val: float = 0.0
    metric1_text: str = "0%"
    metric1_reset: Optional[datetime] = None
    metric1_subtext: str = ""

    metric2_title: str = "WEEKLY 7D"
    metric2_val: float = 0.0
    metric2_text: str = "0%"
    metric2_reset: Optional[datetime] = None
    metric2_subtext: str = ""

    badge1_text: str = ""
    badge2_text: str = ""
    
    last_updated_time: str = ""
    error: Optional[str] = None

class BaseProvider:
    provider_id: str = "base"
    display_name: str = "Base"

    def fetch_usage(self) -> UsageMetrics:
        raise NotImplementedError

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
