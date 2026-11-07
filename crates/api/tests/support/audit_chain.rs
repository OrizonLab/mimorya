//! The audit log chained by company as the tests reach it (ADR-178): the
//! key of the tests' master key, entries written as the application writes
//! them, the chain read back, statements run as the database's owner (past
//! the rule that keeps the seals, when asked), a sealing and a verification.

use mimorya_api::secrets::{PUBLIC_TEST_MASTER_KEYS, SecretCipher};
use mimorya_api::services::audit_chain::{
    ChainBreak, ChainKey, ChainReport, SealedHeads, seal_company, verify_company,
};
use mimorya_db::Database;
use uuid::Uuid;

use super::owner_url;

pub fn key(index: usize) -> ChainKey {
    ChainKey::of(&SecretCipher::from_base64(PUBLIC_TEST_MASTER_KEYS[index]).unwrap()).unwrap()
}

pub async fn append(db: &Database, tenant_id: Uuid, action: &str) -> i64 {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let id = sqlx::query_scalar(
        "insert into audit_log (tenant_id, actor_type, action, outcome, metadata, ip) \
         values ($1, 'user', $2, 'success', '{\"détail\": [1, 2.50, null]}', '192.0.2.7') \
         returning id",
    )
    .bind(tenant_id)
    .bind(action)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// The company's sealed entries, in the chain's order: entry and place.
pub async fn chain(db: &Database, tenant_id: Uuid) -> Vec<(i64, i64)> {
    let mut tx = db.begin(tenant_id).await.unwrap();
    sqlx::query_as(
        "select id, chain_seq from audit_log where chain_seq is not null order by chain_seq",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// Runs `statement` on the entry `id` as the database's owner, who is not
/// held by the application's rights.
pub async fn as_owner(statement: &str, id: i64) -> sqlx::Result<()> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let done = sqlx::query(statement).bind(id).execute(&owner).await;
    owner.close().await;
    done.map(|_| ())
}

/// The same, the rule that keeps the seals dropped for the statement, as
/// an owner set on changing them would.
pub async fn as_owner_over_the_seals(statement: &str, id: i64) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let mut tx = owner.begin().await.unwrap();
    sqlx::query("alter table audit_log disable trigger audit_log_keep_seals")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(statement)
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("alter table audit_log enable trigger audit_log_keep_seals")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    owner.close().await;
}

pub async fn seal(db: &Database, heads: &SealedHeads, tenant_id: Uuid) -> anyhow::Result<usize> {
    seal_company(db, &key(0), heads, tenant_id).await
}

pub async fn verify(db: &Database, tenant_id: Uuid) -> ChainReport {
    verify_company(db, &key(0), tenant_id, &[]).await.unwrap()
}

pub fn intact(report: &ChainReport, verified: i64) {
    assert_eq!(report.broken, 0, "{report:?}");
    assert_eq!(report.verified, verified, "{report:?}");
    assert_eq!(
        report.head.as_ref().map(|head| head.chain_seq),
        Some(verified)
    );
}

pub fn first_break(report: &ChainReport) -> Option<ChainBreak> {
    report.breaks.first().cloned()
}

pub fn altered((entry_id, seq): (i64, i64)) -> Option<ChainBreak> {
    Some(ChainBreak::Altered { seq, entry_id })
}
