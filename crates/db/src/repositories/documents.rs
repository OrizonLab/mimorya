//! Documents a company deposits (ADR-095): read where the reader's filter
//! reaches only (`document_read_filter`, their space and clearance, checked
//! in SQL), deposited with their file and passages in one transaction under
//! the company's quota lock, deleted with both.

use chrono::{DateTime, Utc};
use mimorya_core::documents::{DocumentReadFilter, Passage};
use mimorya_core::memory::UnitFilter;
use sqlx::PgConnection;
use uuid::Uuid;

/// What a reader may see: the company's space ($1), the units' ($2 all,
/// $3 these), up to a clearance ($4).
const VISIBLE: &str = "((scope = 'tenant' and $1) \
    or (scope = 'org_unit' and ($2 or org_unit_id = any($3)))) \
    and array_position(array['public', 'internal', 'confidential', 'restricted'], sensitivity) - 1 <= $4";

const COLUMNS: &str = "id, scope, org_unit_id, title, file_name, kind, sensitivity, byte_size, \
    page_count, uploaded_by_user_id, created_at";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DocumentRow {
    pub id: Uuid,
    pub scope: String,
    pub org_unit_id: Option<Uuid>,
    pub title: String,
    pub file_name: String,
    pub kind: String,
    pub sensitivity: String,
    pub byte_size: i32,
    pub page_count: Option<i32>,
    pub uploaded_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

fn bind_visible<'q, O>(
    query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>,
    filter: &DocumentReadFilter,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments> {
    let (all_units, unit_ids): (bool, Vec<Uuid>) = match &filter.units {
        UnitFilter::All => (true, Vec::new()),
        UnitFilter::Only(ids) => (false, ids.iter().copied().collect()),
    };
    query
        .bind(filter.company)
        .bind(all_units)
        .bind(unit_ids)
        .bind(i32::from(filter.clearance.rank()))
}

/// The documents `filter` reads, the latest first; at most `limit`.
pub async fn list_visible_documents(
    conn: &mut PgConnection,
    filter: &DocumentReadFilter,
    limit: i64,
) -> sqlx::Result<Vec<DocumentRow>> {
    let sql = format!(
        "select {COLUMNS} from documents where {VISIBLE} order by created_at desc, id limit $5"
    );
    bind_visible(sqlx::query_as(&sql), filter)
        .bind(limit)
        .fetch_all(conn)
        .await
}

/// The document `id`, if `filter` reads it.
pub async fn get_visible_document(
    conn: &mut PgConnection,
    filter: &DocumentReadFilter,
    id: Uuid,
) -> sqlx::Result<Option<DocumentRow>> {
    let sql = format!("select {COLUMNS} from documents where {VISIBLE} and id = $5");
    bind_visible(sqlx::query_as(&sql), filter)
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// Locks the company's documents until the end of the transaction, so that
/// two deposits count each other; the bytes they hold.
pub async fn lock_document_quota(conn: &mut PgConnection, tenant_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 95))")
        .bind(tenant_id)
        .execute(&mut *conn)
        .await?;
    document_bytes(conn).await
}

/// The bytes the company's documents hold.
pub async fn document_bytes(conn: &mut PgConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar("select coalesce(sum(byte_size), 0)::bigint from documents")
        .fetch_one(conn)
        .await
}

#[derive(Debug, Clone)]
pub struct NewDocument<'a> {
    pub tenant_id: Uuid,
    pub scope: &'a str,
    pub org_unit_id: Option<Uuid>,
    pub title: &'a str,
    pub file_name: &'a str,
    pub kind: &'a str,
    pub sensitivity: &'a str,
    pub page_count: Option<i32>,
    pub uploaded_by_user_id: Uuid,
    pub content: &'a [u8],
}

/// Records a document and its file; its identifier.
pub async fn insert_document(
    conn: &mut PgConnection,
    document: NewDocument<'_>,
) -> sqlx::Result<Uuid> {
    let byte_size = i32::try_from(document.content.len()).unwrap_or(i32::MAX);
    let id: Uuid = sqlx::query_scalar(
        "insert into documents (tenant_id, scope, org_unit_id, title, file_name, kind, \
         sensitivity, byte_size, page_count, uploaded_by_user_id) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) returning id",
    )
    .bind(document.tenant_id)
    .bind(document.scope)
    .bind(document.org_unit_id)
    .bind(document.title)
    .bind(document.file_name)
    .bind(document.kind)
    .bind(document.sensitivity)
    .bind(byte_size)
    .bind(document.page_count)
    .bind(document.uploaded_by_user_id)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("insert into document_files (document_id, tenant_id, content) values ($1, $2, $3)")
        .bind(id)
        .bind(document.tenant_id)
        .bind(document.content)
        .execute(conn)
        .await?;
    Ok(id)
}

/// Records the passages of the document `document_id`, in order.
pub async fn insert_passages(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    document_id: Uuid,
    passages: &[Passage],
) -> sqlx::Result<()> {
    let ordinals: Vec<i32> = (0..passages.len())
        .map(|ordinal| i32::try_from(ordinal).unwrap_or(i32::MAX))
        .collect();
    let pages: Vec<Option<i32>> = passages
        .iter()
        .map(|passage| passage.page.and_then(|page| i32::try_from(page).ok()))
        .collect();
    let headings: Vec<Option<&str>> = passages.iter().map(|p| p.heading.as_deref()).collect();
    let contents: Vec<&str> = passages.iter().map(|p| p.content.as_str()).collect();
    sqlx::query(
        "insert into document_chunks (tenant_id, document_id, ordinal, page, heading, content) \
         select $1, $2, ordinal, page, heading, content \
         from unnest($3::int4[], $4::int4[], $5::text[], $6::text[]) \
           as passage(ordinal, page, heading, content)",
    )
    .bind(tenant_id)
    .bind(document_id)
    .bind(ordinals)
    .bind(pages)
    .bind(headings)
    .bind(contents)
    .execute(conn)
    .await?;
    Ok(())
}

/// How many passages the document `id` holds.
pub async fn count_passages(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar("select count(*) from document_chunks where document_id = $1")
        .bind(id)
        .fetch_one(conn)
        .await
}

/// The file of the document `id`, as deposited.
pub async fn read_document_file(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<Vec<u8>>> {
    sqlx::query_scalar("select content from document_files where document_id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// Deletes the document `id`, its file and passages with it; whether it was.
pub async fn delete_document(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let deleted = sqlx::query("delete from documents where id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected() > 0)
}
