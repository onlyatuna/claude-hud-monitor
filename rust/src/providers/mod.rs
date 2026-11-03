// src/providers/mod.rs — Provider trait + UsageMetrics + provider registry

pub mod agy;
pub mod base;
pub mod claude;
pub mod codex;
#[cfg(test)]
pub mod stub;

pub use base::{Provider, UsageMetrics};

/// IDs for all providers — order determines card display order.
#[allow(dead_code)]
pub const PROVIDER_IDS: &[&str] = &["claude", "agy", "codex"];
