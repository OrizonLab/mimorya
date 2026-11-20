//! A deposited document's type and confidentiality classified by the
//! decision service (ADR-213, D4): the documents awaiting their
//! classification, requested at a deposit or a replacement while the
//! company's usage is not off; what the classifier reads of one (its title
//! and the passages of its first pages); its level raised only where it is
//! still at the version and level classified; what it leaves for a person
//! who may write the document; and the labels a person sets, at the version
//! and level they were checked at.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Requests the classification of the document `document_id` at `version`,
/// if the company's usage is not off; whether it was. A request for a
/// former version is replaced.
pub async fn request_document_labels(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    document_id: Uuid,
    version: i32,
) -> sqlx::Result<bool> {
    let requested = sqlx::query(
        "insert into document_label_requests (tenant_id, document_id, version) \
         select $1, $2, $3 where exists (select 1 from tenant_decision_usages \
           where usage = 'document_labels' and mode <> 'off') \
         on conflict (tenant_id, document_id) \
           do update set version = excluded.version, requested_at = now()",
    )
    .bind(tenant_id)
    .bind(document_id)
    .bind(version)
    .execute(conn)
    .await?;
    Ok(requested.rows_affected() > 0)
}

/// The active companies with documents awaiting their classification.
pub async fn tenants_awaiting_document_labels(pool: &PgPool) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar("select system_tenants_awaiting_document_labels()")
        .fetch_all(pool)
        .await
}

/// A document awaiting its classification, at a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct LabelRequest {
    pub document_id: Uuid,
    pub version: i32,
}

/// The company's oldest requests, at most `limit`.
pub async fn label_requests(
    conn: &mut PgConnection,
    limit: i64,
) -> sqlx::Result<Vec<LabelRequest>> {
    sqlx::query_as(
        "select document_id, version from document_label_requests \
         order by requested_at, document_id limit $1",
    )
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// Drops the request for `document_id` at `version`, not a later one.
pub async fn drop_label_request(
    conn: &mut PgConnection,
    document_id: Uuid,
    version: i32,
) -> sqlx::Result<()> {
    sqlx::query("delete from document_label_requests where document_id = $1 and version = $2")
        .bind(document_id)
        .bind(version)
        .execute(conn)
        .await?;
    Ok(())
}

/// Drops every request of the company: its usage is off.
pub async fn drop_label_requests(conn: &mut PgConnection) -> sqlx::Result<u64> {
    let dropped = sqlx::query("delete from document_label_requests")
        .execute(conn)
        .await?;
    Ok(dropped.rows_affected())
}

/// What the classifier reads of a document, and whose deposit it is.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LabelledDocument {
    pub title: String,
    pub version: i32,
    pub sensitivity: String,
    pub document_type: Option<String>,
    /// Who deposited or replaced it last: its decision is counted to them.
    pub depositor: Option<Uuid>,
}

/// The document `document_id`, locked until the end of the transaction:
/// what is applied to it is applied to it as read.
pub async fn labelled_document(
    conn: &mut PgConnection,
    document_id: Uuid,
) -> sqlx::Result<Option<LabelledDocument>> {
    sqlx::query_as(
        "select title, version, sensitivity, document_type, \
           coalesce(replaced_by_user_id, uploaded_by_user_id) as depositor \
         from documents where id = $1 for update",
    )
    .bind(document_id)
    .fetch_optional(conn)
    .await
}

/// The passages most read of a document: enough for its first pages.
const FIRST_PASSAGES: i64 = 32;

/// The text of the first `pages` pages of a document (of its start, when
/// it has none), up to `max_chars`; its passages in order, a blank line
/// between them.
pub async fn first_text(
    conn: &mut PgConnection,
    document_id: Uuid,
    pages: i32,
    max_chars: usize,
) -> sqlx::Result<String> {
    let passages: Vec<String> = sqlx::query_scalar(
        "select content from document_chunks \
         where document_id = $1 and (page is null or page <= $2) \
         order by ordinal limit $3",
    )
    .bind(document_id)
    .bind(pages)
    .bind(FIRST_PASSAGES)
    .fetch_all(conn)
    .await?;
    let mut text = String::new();
    for passage in passages {
        if text.chars().count() >= max_chars {
            break;
        }
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&passage);
    }
    Ok(text)
}

/// Raises the document's level from `from` to `to`, where it is still at
/// `version` and `from`; whether it was.
pub async fn raise_document_sensitivity(
    conn: &mut PgConnection,
    document_id: Uuid,
    version: i32,
    from: &str,
    to: &str,
) -> sqlx::Result<bool> {
    let raised = sqlx::query(
        "update documents set sensitivity = $4 \
         where id = $1 and version = $2 and sensitivity = $3",
    )
    .bind(document_id)
    .bind(version)
    .bind(from)
    .bind(to)
    .execute(conn)
    .await?;
    Ok(raised.rows_affected() > 0)
}

/// What a classification leaves for a person who may write the document.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LabelProposal {
    pub document_id: Uuid,
    pub version: i32,
    pub document_type: Option<String>,
    /// A looser level.
    pub sensitivity: Option<String>,
    /// The level before the decision raised it.
    pub raised_from: Option<String>,
    pub engine: String,
}

/// Records what the classification of `proposal.version` leaves, in place
/// of any former one.
pub async fn put_label_proposal(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    proposal: &LabelProposal,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into document_label_proposals \
           (tenant_id, document_id, version, document_type, sensitivity, raised_from, engine) \
         values ($1, $2, $3, $4, $5, $6, $7) \
         on conflict (tenant_id, document_id) do update set version = excluded.version, \
           document_type = excluded.document_type, sensitivity = excluded.sensitivity, \
           raised_from = excluded.raised_from, engine = excluded.engine, proposed_at = now()",
    )
    .bind(tenant_id)
    .bind(proposal.document_id)
    .bind(proposal.version)
    .bind(&proposal.document_type)
    .bind(&proposal.sensitivity)
    .bind(&proposal.raised_from)
    .bind(&proposal.engine)
    .execute(conn)
    .await?;
    Ok(())
}

const PROPOSAL_COLUMNS: &str =
    "document_id, version, document_type, sensitivity, raised_from, engine";

/// What awaits a person for each of `document_ids`.
pub async fn label_proposals(
    conn: &mut PgConnection,
    document_ids: &[Uuid],
) -> sqlx::Result<Vec<LabelProposal>> {
    let sql = format!(
        "select {PROPOSAL_COLUMNS} from document_label_proposals where document_id = any($1)"
    );
    sqlx::query_as(&sql)
        .bind(document_ids)
        .fetch_all(conn)
        .await
}

/// Deletes what awaits a person for the document; whether there was
/// something.
pub async fn drop_label_proposal(conn: &mut PgConnection, document_id: Uuid) -> sqlx::Result<bool> {
    let dropped = sqlx::query("delete from document_label_proposals where document_id = $1")
        .bind(document_id)
        .execute(conn)
        .await?;
    Ok(dropped.rows_affected() > 0)
}

/// Dismisses what awaits a person for the document, if it is the
/// classification of `version`; whether it was.
pub async fn dismiss_label_proposal(
    conn: &mut PgConnection,
    document_id: Uuid,
    version: i32,
) -> sqlx::Result<bool> {
    let dismissed =
        sqlx::query("delete from document_label_proposals where document_id = $1 and version = $2")
            .bind(document_id)
            .bind(version)
            .execute(conn)
            .await?;
    Ok(dismissed.rows_affected() > 0)
}

/// Sets the document's type and level, where it is still at `version` and
/// at the level `seen` its writer was checked at; whether it was.
pub async fn set_document_labels(
    conn: &mut PgConnection,
    document_id: Uuid,
    version: i32,
    seen: &str,
    document_type: Option<&str>,
    sensitivity: &str,
) -> sqlx::Result<bool> {
    let set = sqlx::query(
        "update documents set document_type = $4, sensitivity = $5 \
         where id = $1 and version = $2 and sensitivity = $3",
    )
    .bind(document_id)
    .bind(version)
    .bind(seen)
    .bind(document_type)
    .bind(sensitivity)
    .execute(conn)
    .await?;
    Ok(set.rows_affected() > 0)
}
