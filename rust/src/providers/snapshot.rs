// src/providers/snapshot.rs — fixed-data providers for `--snapshot`.
//
// `--snapshot` renders preview images and must never read credentials or reach the network.
// `HUDWindow::with_providers` launches one background fetch per provider, so the snapshot injects
// these providers instead of the real ones. The dataset lives here, in one place, so the images are
// reproducible (reset countdowns are relative to the `now` the caller passes in).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

use super::base::{Provider, UsageMetrics};

/// The fixed metrics shown for `provider_id` in the preview images; `None` for an unknown id.
pub fn snapshot_metrics(provider_id: &str, now: DateTime<Utc>) -> Option<UsageMetrics> {
    let metrics = match provider_id {
        "claude" => UsageMetrics {
            provider_id: "claude".to_string(),
            provider_name: "Claude".to_string(),
            metric1_title: "SESSION 5H".to_string(),
            metric1_val: Some(42.5),
            metric1_text: "43%".to_string(),
            metric1_reset: Some(now + Duration::hours(2) + Duration::minutes(15)),
            metric2_title: "WEEKLY 7D".to_string(),
            metric2_val: Some(68.0),
            metric2_text: "68%".to_string(),
            metric2_reset: Some(now + Duration::days(3)),
            badge1_text: "1.2x pace".to_string(),
            badge2_text: "Pro Tier".to_string(),
            last_updated_time: "21:50:00".to_string(),
            last_success: Some(now),
            ..Default::default()
        },
        "agy" => UsageMetrics {
            provider_id: "agy".to_string(),
            provider_name: "Antigravity".to_string(),
            metric1_title: "DAILY QUOTA".to_string(),
            metric1_val: Some(18.0),
            metric1_text: "18%".to_string(),
            metric1_reset: Some(now + Duration::hours(18) + Duration::minutes(40)),
            metric2_title: "BURST LIMIT".to_string(),
            metric2_val: Some(5.0),
            metric2_text: "5%".to_string(),
            metric2_reset: None,
            badge1_text: "0.4x pace".to_string(),
            badge2_text: "Enterprise".to_string(),
            last_updated_time: "21:50:02".to_string(),
            last_success: Some(now),
            ..Default::default()
        },
        "codex" => UsageMetrics {
            provider_id: "codex".to_string(),
            provider_name: "OpenAI Codex".to_string(),
            metric1_title: "SESSION 5H".to_string(),
            metric1_val: Some(85.0),
            metric1_text: "85%".to_string(),
            metric1_reset: Some(now + Duration::minutes(45)),
            metric2_title: "MONTHLY".to_string(),
            metric2_val: Some(92.0),
            metric2_text: "92%".to_string(),
            metric2_reset: Some(now + Duration::days(12)),
            badge1_text: "1.8x pace".to_string(),
            badge2_text: "Plus Tier".to_string(),
            last_updated_time: "21:49:58".to_string(),
            last_success: Some(now),
            ..Default::default()
        },
        _ => return None,
    };
    Some(metrics)
}

/// Returns the fixed metrics; touches no file, credential or socket.
struct SnapshotProvider {
    id: &'static str,
}

impl Provider for SnapshotProvider {
    fn provider_id(&self) -> &str {
        self.id
    }

    fn display_name(&self) -> &str {
        self.id
    }

    fn fetch_usage(&self) -> UsageMetrics {
        snapshot_metrics(self.id, Utc::now()).unwrap_or_default()
    }
}

/// One fixed-data provider per id in `PROVIDER_IDS`.
pub fn snapshot_providers() -> HashMap<String, Arc<dyn Provider + Send + Sync>> {
    super::PROVIDER_IDS
        .iter()
        .map(|id| {
            (
                (*id).to_owned(),
                Arc::new(SnapshotProvider { id }) as Arc<dyn Provider + Send + Sync>,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_providers_cover_every_provider_with_fixed_data() {
        let providers = snapshot_providers();
        assert_eq!(providers.len(), super::super::PROVIDER_IDS.len());
        for id in super::super::PROVIDER_IDS {
            let provider = providers.get(*id).expect("provider missing");
            assert_eq!(provider.provider_id(), *id);
            let fetched = provider.fetch_usage();
            let expected = snapshot_metrics(id, Utc::now()).expect("fixture missing");
            assert_eq!(fetched.provider_id, *id);
            assert_eq!(fetched.metric1_val, expected.metric1_val);
            assert_eq!(fetched.metric2_val, expected.metric2_val);
            assert_eq!(fetched.badge2_text, expected.badge2_text);
            assert!(fetched.error.is_none());
        }
    }
}
