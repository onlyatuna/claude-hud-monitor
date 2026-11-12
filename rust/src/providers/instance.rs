// src/providers/instance.rs — an extra account of a built-in provider.
//
// `InstanceProvider` wraps the provider that really fetches (a `ClaudeProvider` pinned to one
// profile) and gives its result the instance's identity: the id (`claude@work`), the row keys
// (`claude:5h` -> `claude@work:5h`) and the header text. The parsers stay untouched, so a default
// instance (`claude`, `agy`, `codex`) behaves exactly as before.

use std::sync::Arc;

use super::base::{Provider, UsageMetrics};

pub struct InstanceProvider {
    inner: Arc<dyn Provider + Send + Sync>,
    id: String,
    /// The built-in provider id whose row keys are rewritten (`claude`).
    base: String,
    /// What the card header and the tray show for this instance.
    name: String,
}

impl InstanceProvider {
    pub fn new(inner: Arc<dyn Provider + Send + Sync>, id: &str, base: &str, name: &str) -> Self {
        Self {
            inner,
            id: id.to_owned(),
            base: base.to_owned(),
            name: name.to_owned(),
        }
    }

    /// `claude:5h` -> `claude@work:5h`; other keys (empty = "the title") stay.
    fn rewrite(&self, key: &mut String) {
        let prefix = format!("{}:", self.base);
        if let Some(rest) = key.strip_prefix(&prefix) {
            *key = format!("{}:{rest}", self.id);
        }
    }
}

impl Provider for InstanceProvider {
    fn provider_id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn fetch_usage(&self) -> UsageMetrics {
        let mut metrics = self.inner.fetch_usage();
        metrics.provider_id = self.id.clone();
        metrics.provider_name = self.name.clone();
        self.rewrite(&mut metrics.metric1_key);
        self.rewrite(&mut metrics.metric2_key);
        for row in &mut metrics.extra_metrics {
            self.rewrite(&mut row.key);
        }
        metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::MetricRow;

    struct Fixed(UsageMetrics);

    impl Provider for Fixed {
        fn provider_id(&self) -> &str {
            "claude"
        }
        fn display_name(&self) -> &str {
            "Claude Code"
        }
        fn fetch_usage(&self) -> UsageMetrics {
            self.0.clone()
        }
    }

    fn wrapped(inner: UsageMetrics) -> InstanceProvider {
        InstanceProvider::new(Arc::new(Fixed(inner)), "claude@work", "claude", "WORK")
    }

    #[test]
    fn keys_and_identity_are_rewritten() {
        let mut fable = MetricRow::new("FABLE 7D", Some(84.0), "84%");
        fable.key = "claude:weekly_scoped:fable".to_owned();
        let m = wrapped(UsageMetrics {
            provider_id: "claude".to_owned(),
            provider_name: "CLAUDE CODE".to_owned(),
            metric1_key: "claude:5h".to_owned(),
            metric2_key: "claude:7d".to_owned(),
            extra_metrics: vec![fable],
            ..Default::default()
        })
        .fetch_usage();
        assert_eq!(m.provider_id, "claude@work");
        assert_eq!(m.provider_name, "WORK");
        assert_eq!(m.metric1_key, "claude@work:5h");
        assert_eq!(m.metric2_key, "claude@work:7d");
        assert_eq!(m.extra_metrics[0].key, "claude@work:weekly_scoped:fable");
    }

    #[test]
    fn error_results_carry_the_instance_id() {
        let m = wrapped(UsageMetrics::error_result(
            "claude",
            "CLAUDE (x)",
            "no login",
            "auth",
        ))
        .fetch_usage();
        assert_eq!(m.provider_id, "claude@work");
        assert_eq!(m.error_code, "auth");
        assert_eq!(m.metric1_key, "");
    }
}
