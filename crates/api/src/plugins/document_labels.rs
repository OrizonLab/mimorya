//! Classifying deposited documents (ADR-213, D4), in the background: a pass
//! at startup, whenever a document is deposited or replaced while the
//! company's usage is not off, and every minute (what a server failure left
//! is tried again then).

use std::time::Duration;

use tokio::task::JoinHandle;

use crate::app::AppState;

const PASS_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn_document_labels(services: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let labels = &services.document_labels;
        let mut interval = tokio::time::interval(PASS_INTERVAL);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                () = labels.woken() => {}
            }
            match labels.classify_pending().await {
                Ok(0) => {}
                Ok(classified) => tracing::info!(classified, "Documents classified"),
                Err(error) => tracing::warn!(
                    error = format!("{error:#}"),
                    "Document classification pass failed"
                ),
            }
        }
    })
}
