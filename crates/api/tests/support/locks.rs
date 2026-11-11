//! Deterministic races: the test holds row locks in its own transaction,
//! waits until the requests under test are queued behind them (as seen in
//! `pg_locks`), then releases them in a known order.

use std::time::Duration;

use mimorya_core::demo::tenant;
use mimorya_db::TenantTx;
use uuid::Uuid;

use super::TestApi;

/// Locks the rows of `table` with these ids until the returned transaction
/// ends.
pub async fn hold_rows(api: &TestApi, table: &str, ids: &[Uuid]) -> TenantTx {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let locked: Vec<Uuid> = sqlx::query_scalar(&format!(
        "select id from {table} where id = any($1) for update"
    ))
    .bind(ids)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(locked.len(), ids.len(), "rows to hold in {table}");
    tx
}

/// Waits until at least `count` lock requests are queued by connections to
/// this test database (other databases of the cluster are ignored).
pub async fn wait_for_waiters(api: &TestApi, count: i64) {
    wait_for(api, count, false).await;
}

/// Waits until at least `count` advisory lock requests are queued by
/// connections to this test database: a row some other work waits on is
/// not counted, so the test's own waiters are.
pub async fn wait_for_advisory_waiters(api: &TestApi, count: i64) {
    wait_for(api, count, true).await;
}

/// Waits for `count` queued requests, advisory ones alone if `advisory`.
async fn wait_for(api: &TestApi, count: i64, advisory: bool) {
    for _ in 0..600 {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        let waiting: i64 = sqlx::query_scalar(
            "select count(*) from pg_locks l join pg_stat_activity a on a.pid = l.pid \
             where not l.granted and a.datname = current_database() \
             and (not $1 or l.locktype = 'advisory')",
        )
        .bind(advisory)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        if waiting >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{count} lock waiter(s) expected");
}

/// Waits for `count` queued requests, then releases the held rows.
pub async fn release_after(api: &TestApi, count: i64, hold: TenantTx) {
    wait_for_waiters(api, count).await;
    hold.commit().await.unwrap();
}

/// Waits for `count` queued advisory lock requests, then releases the
/// held lock.
pub async fn release_after_advisory(api: &TestApi, count: i64, hold: TenantTx) {
    wait_for_advisory_waiters(api, count).await;
    hold.commit().await.unwrap();
}

/// Whether the connection named `name` (its `application_name`) waits on a
/// lock, within five seconds: no other waiting statement of the cluster
/// passes for it. `pg_locks` and another role's application name are read
/// by any role.
pub async fn named_waits(pool: &sqlx::PgPool, name: &str) -> bool {
    for _ in 0..100 {
        let waiting: i64 = sqlx::query_scalar(
            "select count(*) from pg_locks l join pg_stat_activity a on a.pid = l.pid \
             where not l.granted and a.application_name = $1",
        )
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting > 0 {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}
