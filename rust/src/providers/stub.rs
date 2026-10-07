// src/providers/stub.rs — test-only provider that never touches the network or the filesystem.
//
// `HUDWindow::with_providers` launches one background fetch per provider. Tests inject these stubs so
// that building a window does not send real requests (an earlier geometry audit that built many
// windows with the real providers drew HTTP 429 from the live endpoint).

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use super::base::{Provider, UsageMetrics};

pub struct StubProvider {
    id: String,
    fetched: Mutex<Sender<String>>,
}

impl Provider for StubProvider {
    fn provider_id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        &self.id
    }

    fn fetch_usage(&self) -> UsageMetrics {
        let _ = self.fetched.lock().unwrap().send(self.id.clone());
        UsageMetrics {
            provider_id: self.id.clone(),
            provider_name: self.id.clone(),
            ..UsageMetrics::default()
        }
    }
}

/// One stub per id in `PROVIDER_IDS`, plus a receiver that gets the id each time a stub is fetched.
pub fn stub_providers_with_receiver() -> (HashMap<String, Arc<dyn Provider + Send + Sync>>, Receiver<String>) {
    let (tx, rx) = channel();
    let map = super::PROVIDER_IDS
        .iter()
        .map(|id| {
            let stub = StubProvider { id: (*id).to_owned(), fetched: Mutex::new(tx.clone()) };
            ((*id).to_owned(), Arc::new(stub) as Arc<dyn Provider + Send + Sync>)
        })
        .collect();
    (map, rx)
}

pub fn stub_providers() -> HashMap<String, Arc<dyn Provider + Send + Sync>> {
    stub_providers_with_receiver().0
}
