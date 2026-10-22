//! A DNS for the tests (ADR-069): the TXT records a test publishes, or a
//! failure, in place of the system's resolver.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::bail;
use async_trait::async_trait;
use mimorya_api::services::sso::TxtLookup;

#[derive(Default)]
pub struct FakeDns {
    records: Mutex<HashMap<String, Vec<String>>>,
    failing: AtomicBool,
}

impl FakeDns {
    /// A TXT record at `name`, beside those already there.
    pub fn publish(&self, name: &str, value: &str) {
        let mut records = self.records.lock().expect("records");
        records
            .entry(name.to_ascii_lowercase())
            .or_default()
            .push(value.into());
    }

    /// Every lookup fails, as when no server answers.
    pub fn fail(&self, failing: bool) {
        self.failing.store(failing, Ordering::SeqCst);
    }
}

#[async_trait]
impl TxtLookup for FakeDns {
    async fn txt(&self, name: &str) -> anyhow::Result<Vec<String>> {
        if self.failing.load(Ordering::SeqCst) {
            bail!("no DNS server answered");
        }
        let records = self.records.lock().expect("records");
        Ok(records
            .get(&name.to_ascii_lowercase())
            .cloned()
            .unwrap_or_default())
    }
}
