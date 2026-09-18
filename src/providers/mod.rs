// src/providers/mod.rs — Provider trait + UsageMetrics + provider registry

pub mod base;
pub mod claude;
pub mod agy;
pub mod codex;

pub use base::{Provider, UsageMetrics};
pub use claude::ClaudeProvider;
pub use agy::AgyProvider;
pub use codex::CodexProvider;

/// IDs for all providers — order determines card display order.
pub const PROVIDER_IDS: &[&str] = &["claude", "agy", "codex"];
