//! The audit retention tests' data (ADR-198): a company whose chain starts
//! empty, entries of any age written as the application writes them, the
//! seal a place holds, where the chain starts once its oldest places went,
//! the purge as the retention pass makes it, with the tests' key, and
//! statements of the database's owner.

use chrono::{DateTime, Utc};
use mimorya_api::services::audit_chain::purge_old_places;
use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{ChainHead, ChainStart, audit_chain_start};
use uuid::Uuid;

use super::audit_chain::key;
use super::{TestApi, companies, owner_url};

/// A company made for the test, its audit log emptied of what its making
/// wrote, still unsealed: its chain starts with what the test writes.
pub async fn empty_chain(api: &TestApi, slug: &str) -> Uuid {
    let company = companies::create(api, slug, &format!("admin@{slug}.example")).await;
    unsealed_gone(company.tenant_id).await;
    company.tenant_id
}

/// Deletes the company's entries still to seal, as the database's owner.
pub async fn unsealed_gone(tenant_id: Uuid) {
    owner_runs(&format!(
        "delete from audit_log where tenant_id = '{tenant_id}' and chain_seq is null"
    ))
    .await
    .unwrap();
}

/// Runs `statement` as the database's owner, who is not held by the
/// application's rights.
pub async fn owner_runs(statement: &str) -> sqlx::Result<()> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let done = sqlx::query(statement).execute(&owner).await;
    owner.close().await;
    done.map(|_| ())
}

/// What the purge deleted of the company `tenant_id`, `batch` places at
/// most in all, with the tests' key: entries, through.
pub async fn purged_with(db: &Database, tenant_id: Uuid, batch: i32) -> Option<(i32, i64)> {
    purged_at(db, tenant_id, batch, Utc::now()).await
}

/// The same, the process's clock at `now`.
pub async fn purged_at(
    db: &Database,
    tenant_id: Uuid,
    batch: i32,
    now: DateTime<Utc>,
) -> Option<(i32, i64)> {
    let lost = purge_old_places(db, &key(0), batch, now).await.unwrap();
    lost.iter()
        .find(|company| company.tenant_id == tenant_id)
        .map(|company| (company.entries, company.through_seq))
}

/// Where the company's chain starts, as the database holds it.
pub async fn start_of(db: &Database, tenant_id: Uuid) -> Option<ChainStart> {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let start = audit_chain_start(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    start
}

/// `count` entries of the company, `action`, that occurred `years` ago.
pub async fn entries(db: &Database, tenant_id: Uuid, action: &str, count: i32, years: i32) {
    let mut tx = db.begin(tenant_id).await.unwrap();
    sqlx::query(
        "insert into audit_log (tenant_id, occurred_at, actor_type, action, outcome, metadata) \
         select $1, now() - make_interval(years => $4), 'user', $2, 'success', '{}' \
         from generate_series(1, $3)",
    )
    .bind(tenant_id)
    .bind(action)
    .bind(count)
    .bind(years)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// The place `seq` of the company's chain and its seal, as an anchor.
pub async fn place(db: &Database, tenant_id: Uuid, seq: i64) -> ChainHead {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let head = sqlx::query_as("select chain_seq, chain_hash from audit_log where chain_seq = $1")
        .bind(seq)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    head
}

/// The places of the company's chain, in its order.
pub async fn places(db: &Database, tenant_id: Uuid) -> Vec<i64> {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let places = sqlx::query_scalar(
        "select chain_seq from audit_log where chain_seq is not null order by chain_seq",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    places
}

/// The metadata of the company's `audit.purged` entries, in their order.
pub async fn purges(db: &Database, tenant_id: Uuid) -> Vec<serde_json::Value> {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let purges = sqlx::query_scalar(
        "select metadata from audit_log where action = 'audit.purged' order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    purges
}
