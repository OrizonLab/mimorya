//! Another process's side of the run events relay (ADR-083): an event it
//! recorded without its notice reaching this process (sent while nobody
//! listened), the notices it sends, and the listener's connection cut.

use std::time::Duration;

use mimorya_api::services::BusEvent;
use mimorya_api::services::event_bus::{Notice, Subscription};
use mimorya_db::notifications::notify_run_events;
use mimorya_db::repositories::run_events::append_run_event;
use serde_json::json;
use uuid::Uuid;

use super::{app_url, owner_url};

/// The relay's listener connection, as an operator finds it.
const LISTENER: &str = "mimorya-run-events";

/// Records an event of `run_id` whose notice nobody receives; its number.
pub async fn record_unannounced(tenant_id: Uuid, run_id: Uuid) -> i32 {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let mut conn = owner.acquire().await.unwrap();
    let data = json!({ "status": "running" });
    let row = append_run_event(&mut conn, tenant_id, run_id, "run_resumed", &data)
        .await
        .unwrap();
    drop(conn);
    owner.close().await;
    row.seq
}

/// Sends `payload` on the run events channel, as another process would.
pub async fn announce(payload: &str) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    notify_run_events(&owner, payload).await.unwrap();
    owner.close().await;
}

/// Announces the event `seq` of `run_id`, from another process.
pub async fn announce_event(run_id: Uuid, seq: i32) {
    announce(&Notice::event(Uuid::new_v4(), run_id, seq)).await;
}

/// Announces a text delta of `run_id` written after its event `after`.
pub async fn announce_delta(run_id: Uuid, after: i32, text: &str) {
    for payload in Notice::deltas(Uuid::new_v4(), run_id, Some(after), text) {
        announce(&payload).await;
    }
}

/// Waits until a relay listens to this test database.
pub async fn wait_for_listener() {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    for _ in 0..500 {
        let listening: i64 = sqlx::query_scalar(
            "select count(*) from pg_stat_activity \
             where application_name = $1 and datname = current_database()",
        )
        .bind(LISTENER)
        .fetch_one(&owner)
        .await
        .unwrap();
        if listening > 0 {
            owner.close().await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no relay listens");
}

/// Cuts the relays' listener connections to this test database, as the
/// application's role, whose connections they are.
pub async fn cut_listeners() {
    let app = sqlx::PgPool::connect(app_url()).await.unwrap();
    let cut: Vec<bool> = sqlx::query_scalar(
        "select pg_terminate_backend(pid) from pg_stat_activity \
         where application_name = $1 and datname = current_database()",
    )
    .bind(LISTENER)
    .fetch_all(&app)
    .await
    .unwrap();
    assert!(!cut.is_empty() && cut.iter().all(|cut| *cut), "{cut:?}");
    app.close().await;
}

/// The next event `events` receives within `seconds`, if any.
pub async fn next_event(events: &mut Subscription, seconds: f64) -> Option<BusEvent> {
    let wait = Duration::from_secs_f64(seconds);
    tokio::time::timeout(wait, events.recv())
        .await
        .ok()
        .flatten()
}
