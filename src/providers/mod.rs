// src/providers/mod.rs — Provider trait + UsageMetrics + provider registry

pub mod agy;
pub mod base;
pub mod claude;
pub mod codex;

pub use agy::AgyProvider;
pub use base::{Provider, UsageMetrics};
pub use claude::ClaudeProvider;
pub use codex::CodexProvider;

/// IDs for all providers — order determines card display order.
pub const PROVIDER_IDS: &[&str] = &["claude", "agy", "codex"];
