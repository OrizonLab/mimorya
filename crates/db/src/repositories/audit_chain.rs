//! The audit log chained by company (ADR-178): the entries still to seal,
//! the chain's last place, the seals set, the sealed entries read back in
//! the chain's order to verify them, and where the chain starts once its
//! oldest places were deleted (ADR-198). An entry's content is read as the
//! database writes it as text, so that its seal never depends on how a
//! client decodes a value: identifiers and addresses as their text, the
//! time in microseconds since 1970, the metadata as its canonical `jsonb`
//! text.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// An entry's content as sealed, with its place and seal once it has them.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ChainEntry {
    pub id: i64,
    pub chain_seq: Option<i64>,
    pub chain_hash: Option<Vec<u8>>,
    pub tenant_id: String,
    pub occurred_us: i64,
    pub actor_type: String,
    pub actor_user_id: Option<String>,
    pub on_behalf_of_user_id: Option<String>,
    pub run_id: Option<String>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub outcome: String,
    pub reason: Option<String>,
    pub metadata: String,
    pub ip: Option<String>,
    pub request_id: Option<String>,
}

const CONTENT: &str = "id, chain_seq, chain_hash, tenant_id::text as tenant_id, \
     (extract(epoch from occurred_at) * 1000000)::bigint as occurred_us, actor_type, \
     actor_user_id::text as actor_user_id, on_behalf_of_user_id::text as on_behalf_of_user_id, \
     run_id::text as run_id, action, resource_type, resource_id, outcome, reason, \
     metadata::text as metadata, ip::text as ip, request_id";

/// The chain's last place in a company and its seal.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ChainHead {
    pub chain_seq: i64,
    pub chain_hash: Vec<u8>,
}

/// Where a company's chain starts once its oldest places were deleted
/// (ADR-198): its first place, and the seal of the place before it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ChainStart {
    pub first_seq: i64,
    pub previous_hash: Vec<u8>,
    /// How many places were deleted in all.
    pub purged_entries: i64,
}

/// Where the company's chain starts, if its oldest places were deleted.
pub async fn audit_chain_start(conn: &mut PgConnection) -> sqlx::Result<Option<ChainStart>> {
    sqlx::query_as("select first_seq, previous_hash, purged_entries from audit_chain_starts")
        .fetch_optional(conn)
        .await
}

/// The companies whose audit log has entries, or, with `unsealed`, entries
/// still to seal, through a narrowly scoped database function: each is
/// then read in its own company.
pub async fn audit_log_tenants(pool: &PgPool, unsealed: bool) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar("select tenant_id from system_audit_log_tenants($1)")
        .bind(unsealed)
        .fetch_all(pool)
        .await
}

/// Holds the company's chain until the transaction ends: the seals
/// computed after the head read under it follow it still when set.
pub async fn lock_audit_chain(conn: &mut PgConnection, tenant_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "select pg_advisory_xact_lock(hashtextextended('mimorya.audit_chain:' || $1::text, 0))",
    )
    .bind(tenant_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// The company's last sealed place, if any.
pub async fn audit_chain_head(conn: &mut PgConnection) -> sqlx::Result<Option<ChainHead>> {
    sqlx::query_as(
        "select chain_seq, chain_hash from audit_log where chain_seq is not null \
         order by chain_seq desc limit 1",
    )
    .fetch_optional(conn)
    .await
}

/// The seal the company's chain holds at the place `seq`, if any.
pub async fn audit_seal_at(conn: &mut PgConnection, seq: i64) -> sqlx::Result<Option<Vec<u8>>> {
    sqlx::query_scalar("select chain_hash from audit_log where chain_seq = $1")
        .bind(seq)
        .fetch_optional(conn)
        .await
}

/// The company's entries still to seal, in the order they were written, at
/// most `limit`.
pub async fn unsealed_audit_entries(
    conn: &mut PgConnection,
    limit: i64,
) -> sqlx::Result<Vec<ChainEntry>> {
    sqlx::query_as(&format!(
        "select {CONTENT} from audit_log where chain_seq is null order by id limit $1"
    ))
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// How many of the company's entries are still to seal.
pub async fn count_unsealed_audit_entries(conn: &mut PgConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar("select count(*) from audit_log where chain_seq is null")
        .fetch_one(conn)
        .await
}

/// A seal to set: the entry, its place and its seal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seal {
    pub id: i64,
    pub seq: i64,
    pub hash: Vec<u8>,
}

/// Sets the seals of the company `tenant_id`, in order after its last
/// place; fails, and sets none, if one does not follow or its entry is not
/// an unsealed one of the company.
pub async fn seal_audit_entries(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    seals: &[Seal],
) -> sqlx::Result<i32> {
    let ids: Vec<i64> = seals.iter().map(|seal| seal.id).collect();
    let seqs: Vec<i64> = seals.iter().map(|seal| seal.seq).collect();
    let hashes: Vec<&[u8]> = seals.iter().map(|seal| seal.hash.as_slice()).collect();
    sqlx::query_scalar("select system_seal_audit_entries($1, $2, $3, $4)")
        .bind(tenant_id)
        .bind(ids)
        .bind(seqs)
        .bind(hashes)
        .fetch_one(conn)
        .await
}

/// The company's sealed entries after the place `after` (and, at that
/// place, after the entry `after_id`), in the chain's order, at most
/// `limit`.
pub async fn sealed_audit_entries(
    conn: &mut PgConnection,
    after: (i64, i64),
    limit: i64,
) -> sqlx::Result<Vec<ChainEntry>> {
    sqlx::query_as(&format!(
        "select {CONTENT} from audit_log where chain_seq is not null \
         and (chain_seq, id) > ($1, $2) order by chain_seq, id limit $3"
    ))
    .bind(after.0)
    .bind(after.1)
    .bind(limit)
    .fetch_all(conn)
    .await
}
