//! Memory persistence and listing. Visibility is applied in SQL from the
//! filter computed by the core memory rules (pre-filtering): rows outside the
//! subject's reach are never loaded, let alone ranked. The search lives in
//! `memory_search.rs`, the vectors in `memory_embeddings.rs`.

use chrono::{DateTime, Utc};
use mimorya_core::authz::Sensitivity;
use mimorya_core::memory::{MemoryKind, MemoryReadFilter, MemoryScope, MemoryStatus, UnitFilter};

pub use super::memory_search::{Meaning, MemorySearchHit, search_memory, to_or_tsquery};
pub use mimorya_core::memory::MemorySource;
use sqlx::PgConnection;
use sqlx::types::Json;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MemoryRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub scope: String,
    pub owner_user_id: Option<Uuid>,
    pub org_unit_id: Option<Uuid>,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub sensitivity: String,
    pub status: String,
    #[sqlx(json)]
    pub source: MemorySource,
    pub created_by_user_id: Option<Uuid>,
    pub created_by_run_id: Option<Uuid>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub use_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const MEMORY_COLUMNS: &str = "id, tenant_id, scope, owner_user_id, org_unit_id, kind, title, content, \
    sensitivity, status, source, created_by_user_id, created_by_run_id, last_used_at, use_count, \
    created_at, updated_at";

/// Visibility predicate. Parameters: $1 owner, $2 shared, $3 all units,
/// $4 unit ids, $5 clearance rank, $6 the units shared with the subject's
/// (ADR-143) and $7 the highest rank read in each.
pub(super) const VISIBLE: &str = "((scope = 'user' and owner_user_id = $1) \
    or ($2 and ((scope = 'org_unit' and ($3 or org_unit_id = any($4))) or scope in ('tenant', 'experience')) \
        and array_position(array['public', 'internal', 'confidential', 'restricted'], sensitivity) - 1 <= $5) \
    or ($2 and scope = 'org_unit' and exists (select 1 from unnest($6::uuid[], $7::int[]) as shared(unit, cap) \
        where shared.unit = org_unit_id \
        and array_position(array['public', 'internal', 'confidential', 'restricted'], sensitivity) - 1 <= shared.cap)))";

pub(super) const ACTIVE: &str = "status = 'active' and (expires_at is null or expires_at > now())";

pub(super) fn bind_visibility<'q, O>(
    query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>,
    filter: &MemoryReadFilter,
    clearance: Sensitivity,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments> {
    let (all_units, unit_ids): (bool, Vec<Uuid>) = match &filter.org_units {
        UnitFilter::All => (true, Vec::new()),
        UnitFilter::Only(ids) => (false, ids.iter().copied().collect()),
    };
    let (shared_units, shared_caps): (Vec<Uuid>, Vec<i32>) = (filter.from_shares.iter())
        .map(|&(unit, cap)| (unit, i32::from(cap.rank())))
        .unzip();
    query
        .bind(filter.owner_user_id)
        .bind(filter.shared)
        .bind(all_units)
        .bind(unit_ids)
        .bind(i32::from(clearance.rank()))
        .bind(shared_units)
        .bind(shared_caps)
}

/// Memories visible to the subject, newest first (memory management screen).
pub async fn list_visible_memory(
    conn: &mut PgConnection,
    filter: &MemoryReadFilter,
    clearance: Sensitivity,
    include_proposed: bool,
    limit: i64,
) -> sqlx::Result<Vec<MemoryRow>> {
    let status = if include_proposed {
        "status in ('active', 'proposed')"
    } else {
        "status = 'active'"
    };
    let sql = format!(
        "select {MEMORY_COLUMNS} from memory_items where {status} and {VISIBLE} \
         order by updated_at desc limit $8"
    );
    bind_visibility(sqlx::query_as(&sql), filter, clearance)
        .bind(limit)
        .fetch_all(conn)
        .await
}

#[derive(Debug, Clone)]
pub struct NewMemory {
    pub tenant_id: Uuid,
    pub scope: MemoryScope,
    pub owner_user_id: Option<Uuid>,
    pub org_unit_id: Option<Uuid>,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    pub sensitivity: Sensitivity,
    pub status: MemoryStatus,
    pub source: MemorySource,
    pub created_by_user_id: Option<Uuid>,
    pub created_by_run_id: Option<Uuid>,
}

pub async fn create_memory(conn: &mut PgConnection, input: NewMemory) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into memory_items (tenant_id, scope, owner_user_id, org_unit_id, kind, title, content, \
         sensitivity, status, source, created_by_user_id, created_by_run_id) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) returning id",
    )
    .bind(input.tenant_id)
    .bind(input.scope.as_str())
    .bind(input.owner_user_id)
    .bind(input.org_unit_id)
    .bind(input.kind.as_str())
    .bind(input.title)
    .bind(input.content)
    .bind(input.sensitivity.as_str())
    .bind(input.status.as_str())
    .bind(Json(input.source))
    .bind(input.created_by_user_id)
    .bind(input.created_by_run_id)
    .fetch_one(conn)
    .await
}

pub async fn get_memory(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<MemoryRow>> {
    sqlx::query_as(&format!(
        "select {MEMORY_COLUMNS} from memory_items where id = $1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}

pub async fn mark_memory_used(conn: &mut PgConnection, ids: &[Uuid]) -> sqlx::Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "update memory_items set last_used_at = now(), use_count = use_count + 1 where id = any($1)",
    )
    .bind(ids)
    .execute(conn)
    .await?;
    Ok(())
}

/// Settles a proposal (`active` or `archived`) unless it was settled
/// meanwhile: two concurrent decisions cannot both apply. Returns whether the
/// proposal was settled.
pub async fn settle_memory_proposal(
    conn: &mut PgConnection,
    id: Uuid,
    status: MemoryStatus,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update memory_items set status = $2, updated_at = now() where id = $1 and status = 'proposed'",
    )
    .bind(id)
    .bind(status.as_str())
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Hard delete (right to erasure); the audit log keeps the trace without the content.
pub async fn delete_memory(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<()> {
    sqlx::query("delete from memory_items where id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}
