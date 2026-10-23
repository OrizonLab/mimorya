//! The invalidations channel (ADR-085): this process's announcements sent
//! in the order they were made, the others' applied here as they come. Once
//! connected to the channel, at the start or again after the connection was
//! lost, the caches are emptied: what was announced while none listened is
//! lost. Connectors keep their instances, whose version each use checks. A
//! connection lost is made again a second later; one that cannot be made is
//! tried again later, waiting twice as long each time up to 30 s.

use std::time::Duration;

use mimorya_db::Database;
use mimorya_db::notifications::{listen_invalidations, notify_invalidation};
use sqlx::postgres::PgListener;
use uuid::Uuid;

use super::{Invalidation, Outgoing, announced_by_another, payload};
use crate::app::AppState;

/// How long before connecting again, doubled after each failure up to
/// `RETRY_AT_MOST`.
const RETRY: Duration = Duration::from_secs(1);
const RETRY_AT_MOST: Duration = Duration::from_secs(30);

/// Sends this process's announcements and applies the others', until the
/// task is stopped.
pub async fn share_invalidations(services: AppState, outgoing: Outgoing) {
    let origin = outgoing.origin;
    tokio::join!(
        send_all(services.db.clone(), outgoing),
        apply_others(services, origin)
    );
}

async fn send_all(db: Database, mut outgoing: Outgoing) {
    while let Some(invalidation) = outgoing.receiver.recv().await {
        let payload = payload(outgoing.origin, invalidation);
        if let Err(error) = notify_invalidation(db.pool(), &payload).await {
            tracing::warn!(
                ?invalidation,
                error = format!("{error:#}"),
                "Invalidation not announced"
            );
        }
    }
}

async fn apply_others(services: AppState, origin: Uuid) {
    let url = services.config.database_app_url.clone();
    let mut retry = RETRY;
    loop {
        match listen_invalidations(&url).await {
            Ok(listener) => {
                retry = RETRY;
                forget_all(&services);
                services.invalidations.set_listening(true);
                hear(&services, origin, listener).await;
                services.invalidations.set_listening(false);
            }
            Err(error) => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "Invalidations listener not connected"
                );
            }
        }
        tokio::time::sleep(retry).await;
        retry = (retry * 2).min(RETRY_AT_MOST);
    }
}

/// Applies what the other processes announce until the connection is lost.
async fn hear(services: &AppState, origin: Uuid, mut listener: PgListener) {
    loop {
        match listener.try_recv().await {
            Ok(Some(notification)) => {
                if let Some(invalidation) = announced_by_another(origin, notification.payload()) {
                    apply(services, invalidation).await;
                }
            }
            Ok(None) => {
                tracing::warn!("Invalidations listener lost");
                return;
            }
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), "Invalidations listener lost");
                return;
            }
        }
    }
}

/// Applies another process's change here, without announcing it again.
async fn apply(services: &AppState, invalidation: Invalidation) {
    match invalidation {
        Invalidation::Access(tenant_id) => services.access.forget(tenant_id),
        Invalidation::Llm(tenant_id) => services.llm.forget(tenant_id),
        Invalidation::Embeddings(tenant_id) => services.embeddings.forget(tenant_id),
        Invalidation::Indexing => services.embeddings.wake_here(),
        Invalidation::Connector(id) => services.connectors.forget(id).await,
        Invalidation::ConnectorRetry(id) => services.connectors.retry_here(id).await,
    }
}

fn forget_all(services: &AppState) {
    services.access.forget_all();
    services.llm.forget_all();
    services.embeddings.forget_all();
}
