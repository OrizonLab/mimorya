//! The audit retention tests' data (ADR-198): a company whose chain starts
//! empty, entries of any age written as the application writes them, the
//! seal a place holds, and where the chain starts once its oldest places
//! went.

use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::ChainHead;
use uuid::Uuid;

use super::{TestApi, companies, owner_url};

/// A company made for the test, its audit log emptied of what its making
/// wrote, still unsealed: its chain starts with what the test writes.
pub async fn empty_chain(api: &TestApi, slug: &str) -> Uuid {
    let company = companies::create(api, slug, &format!("admin@{slug}.example")).await;
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("delete from audit_log where tenant_id = $1 and chain_seq is null")
        .bind(company.tenant_id)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    company.tenant_id
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
