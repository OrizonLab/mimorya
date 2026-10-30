//! A company's units' knowledge shared with other units (ADR-143): listed,
//! made and removed by its administrators of access policies, and loaded
//! with the rest of its authorization (`load_knowledge_shares`), where the
//! read filters find what each member reads through them.

use chrono::{DateTime, Utc};
use mimorya_core::knowledge_shares::KnowledgeShare;
use sqlx::PgConnection;
use uuid::Uuid;

/// A share, as kept.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnowledgeShareRow {
    pub id: Uuid,
    pub source_org_unit_id: Uuid,
    pub target_org_unit_id: Uuid,
    pub include_descendants: bool,
    pub shares_memory: bool,
    pub shares_documents: bool,
    pub max_sensitivity: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl KnowledgeShareRow {
    /// The share the read filters apply; `None` for a sensitivity the base
    /// does not hold (its constraint names them all).
    pub fn share(&self) -> Option<KnowledgeShare> {
        Some(KnowledgeShare {
            source_unit_id: self.source_org_unit_id,
            target_unit_id: self.target_org_unit_id,
            include_descendants: self.include_descendants,
            memory: self.shares_memory,
            documents: self.shares_documents,
            ceiling: self.max_sensitivity.parse().ok()?,
            expires_at: self.expires_at,
        })
    }
}

const COLUMNS: &str = "id, source_org_unit_id, target_org_unit_id, include_descendants, \
    shares_memory, shares_documents, max_sensitivity, expires_at, created_by_user_id, created_at";

/// A share to make.
pub struct NewKnowledgeShare {
    pub tenant_id: Uuid,
    pub share: KnowledgeShare,
    pub created_by_user_id: Uuid,
}

/// The company's shares, the latest first.
pub async fn list_knowledge_shares(
    conn: &mut PgConnection,
) -> sqlx::Result<Vec<KnowledgeShareRow>> {
    sqlx::query_as(&format!(
        "select {COLUMNS} from knowledge_shares order by created_at desc, id"
    ))
    .fetch_all(conn)
    .await
}

/// The company's shares the read filters apply.
pub async fn load_knowledge_shares(conn: &mut PgConnection) -> sqlx::Result<Vec<KnowledgeShare>> {
    let rows = list_knowledge_shares(conn).await?;
    Ok(rows.iter().filter_map(KnowledgeShareRow::share).collect())
}

/// Whether the unit `source`'s knowledge is shared with `target` already.
pub async fn knowledge_share_exists(
    conn: &mut PgConnection,
    (source, target): (Uuid, Uuid),
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "select exists (select 1 from knowledge_shares \
         where source_org_unit_id = $1 and target_org_unit_id = $2)",
    )
    .bind(source)
    .bind(target)
    .fetch_one(conn)
    .await
}

/// Makes the share `new`; its identifier.
pub async fn create_knowledge_share(
    conn: &mut PgConnection,
    new: NewKnowledgeShare,
) -> sqlx::Result<Uuid> {
    let share = &new.share;
    sqlx::query_scalar(
        "insert into knowledge_shares (tenant_id, source_org_unit_id, target_org_unit_id, \
         include_descendants, shares_memory, shares_documents, max_sensitivity, expires_at, \
         created_by_user_id) values ($1, $2, $3, $4, $5, $6, $7, $8, $9) returning id",
    )
    .bind(new.tenant_id)
    .bind(share.source_unit_id)
    .bind(share.target_unit_id)
    .bind(share.include_descendants)
    .bind(share.memory)
    .bind(share.documents)
    .bind(share.ceiling.as_str())
    .bind(share.expires_at)
    .bind(new.created_by_user_id)
    .fetch_one(conn)
    .await
}

/// Removes the share `id`; what it was, if it was one.
pub async fn delete_knowledge_share(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<KnowledgeShareRow>> {
    sqlx::query_as(&format!(
        "delete from knowledge_shares where id = $1 returning {COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}
