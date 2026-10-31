//! A document replaced by a new version of its file (ADR-150): the same
//! document, its space, sensitivity, depositor and first deposit kept; its
//! file, name, title, kind, size, pages and passages the new version's,
//! the former ones deleted, their vectors with them. A replacement names
//! the version it replaces: none is made over another made in between.

use mimorya_core::documents::Passage;
use sqlx::PgConnection;
use uuid::Uuid;

use super::documents::insert_passages;

/// A new version of the file of the document `document_id`.
#[derive(Debug, Clone)]
pub struct NewVersion<'a> {
    pub tenant_id: Uuid,
    pub document_id: Uuid,
    /// The version it replaces.
    pub replaces: i32,
    pub title: &'a str,
    pub file_name: &'a str,
    pub kind: &'a str,
    pub page_count: Option<i32>,
    pub replaced_by_user_id: Uuid,
    pub content: &'a [u8],
    pub passages: &'a [Passage],
}

/// Replaces the file and passages of the document, if it is still at the
/// version replaced: its new version; `None` if it is not (or is gone).
pub async fn replace_document_file(
    conn: &mut PgConnection,
    version: NewVersion<'_>,
) -> sqlx::Result<Option<i32>> {
    let byte_size = i32::try_from(version.content.len()).unwrap_or(i32::MAX);
    let replaced: Option<i32> = sqlx::query_scalar(
        "update documents set title = $3, file_name = $4, kind = $5, byte_size = $6, \
         page_count = $7, version = version + 1, replaced_at = now(), \
         replaced_by_user_id = $8 \
         where id = $1 and version = $2 returning version",
    )
    .bind(version.document_id)
    .bind(version.replaces)
    .bind(version.title)
    .bind(version.file_name)
    .bind(version.kind)
    .bind(byte_size)
    .bind(version.page_count)
    .bind(version.replaced_by_user_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(replaced) = replaced else {
        return Ok(None);
    };
    sqlx::query("update document_files set content = $2 where document_id = $1")
        .bind(version.document_id)
        .bind(version.content)
        .execute(&mut *conn)
        .await?;
    sqlx::query("delete from document_chunks where document_id = $1")
        .bind(version.document_id)
        .execute(&mut *conn)
        .await?;
    insert_passages(
        conn,
        version.tenant_id,
        version.document_id,
        version.passages,
    )
    .await?;
    Ok(Some(replaced))
}
