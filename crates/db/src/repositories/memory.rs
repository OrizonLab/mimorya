//! Memory persistence and retrieval. Visibility is applied in SQL from the
//! filter computed by the core memory rules (pre-filtering): rows outside the
//! subject's reach are never loaded, let alone ranked.

use chrono::{DateTime, Utc};
use mimorya_core::authz::Sensitivity;
use mimorya_core::memory::{MemoryKind, MemoryReadFilter, MemoryScope, UnitFilter};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use sqlx::types::Json;
use uuid::Uuid;

/// Where a memory comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySource {
    /// `conversation`, `run`, `document` or `manual`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl MemorySource {
    pub fn manual(label: Option<&str>) -> Self {
        Self {
            kind: "manual".into(),
            id: None,
            label: label.map(Into::into),
        }
    }
}

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
/// $4 unit ids, $5 clearance rank.
const VISIBLE: &str = "((scope = 'user' and owner_user_id = $1) \
    or ($2 and ((scope = 'org_unit' and ($3 or org_unit_id = any($4))) or scope in ('tenant', 'experience')) \
        and array_position(array['public', 'internal', 'confidential', 'restricted'], sensitivity) - 1 <= $5))";

const ACTIVE: &str = "status = 'active' and (expires_at is null or expires_at > now())";

fn bind_visibility<'q, O>(
    query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>,
    filter: &MemoryReadFilter,
    clearance: Sensitivity,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments> {
    let (all_units, unit_ids): (bool, Vec<Uuid>) = match &filter.org_units {
        UnitFilter::All => (true, Vec::new()),
        UnitFilter::Only(ids) => (false, ids.iter().copied().collect()),
    };
    query
        .bind(filter.owner_user_id)
        .bind(filter.shared)
        .bind(all_units)
        .bind(unit_ids)
        .bind(i32::from(clearance.rank()))
}

/// OR-query over the significant words of a natural-language request.
pub fn to_or_tsquery(query: &str) -> Option<String> {
    let mut words: Vec<String> = Vec::new();
    for word in query.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if word.chars().count() > 2 && !words.iter().any(|w| w == word) {
            words.push(word.to_string());
        }
        if words.len() == 24 {
            break;
        }
    }
    (!words.is_empty()).then(|| words.join(" | "))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MemorySearchHit {
    pub id: Uuid,
    pub scope: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub org_unit_id: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
    pub score: f32,
}

pub async fn search_memory(
    conn: &mut PgConnection,
    filter: &MemoryReadFilter,
    query: &str,
    limit: i64,
    clearance: Sensitivity,
) -> sqlx::Result<Vec<MemorySearchHit>> {
    let Some(ts_query) = to_or_tsquery(query) else {
        return Ok(Vec::new());
    };
    let sql = format!(
        "select id, scope, kind, title, content, org_unit_id, updated_at, \
         ts_rank_cd(content_tsv, to_tsquery('french', $6)) as score \
         from memory_items where {ACTIVE} and {VISIBLE} and content_tsv @@ to_tsquery('french', $6) \
         order by score desc, updated_at desc limit $7"
    );
    bind_visibility(sqlx::query_as(&sql), filter, clearance)
        .bind(ts_query)
        .bind(limit)
        .fetch_all(conn)
        .await
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
         order by updated_at desc limit $6"
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
    /// `active` or `proposed`.
    pub status: &'static str,
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
    .bind(input.status)
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

pub async fn set_memory_status(
    conn: &mut PgConnection,
    id: Uuid,
    status: &str,
) -> sqlx::Result<()> {
    sqlx::query("update memory_items set status = $2, updated_at = now() where id = $1")
        .bind(id)
        .bind(status)
        .execute(conn)
        .await?;
    Ok(())
}

/// Hard delete (right to erasure); the audit log keeps the trace without the content.
pub async fn delete_memory(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<()> {
    sqlx::query("delete from memory_items where id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}
