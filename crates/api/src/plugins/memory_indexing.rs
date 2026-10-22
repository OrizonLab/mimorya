//! Indexing memories by meaning (ADR-074), in the background: a pass at
//! startup, whenever a memory is written or approved or a company changes
//! its model, and every minute (what failed is retried then).

use std::time::Duration;

use tokio::task::JoinHandle;

use crate::app::AppState;

const PASS_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn_memory_indexing(services: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let embeddings = &services.embeddings;
        let mut interval = tokio::time::interval(PASS_INTERVAL);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                () = embeddings.woken() => {}
            }
            match embeddings.index_pending().await {
                Ok(0) => {}
                Ok(embedded) => tracing::info!(embedded, "Memories embedded"),
                Err(error) => tracing::warn!(error = %error, "Memory indexing pass failed"),
            }
        }
    })
}
