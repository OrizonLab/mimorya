//! The company's documents as a person reaches them (ADR-095): listed,
//! read, downloaded and deleted where `document:read` reaches, at or under
//! their clearance; anything else answers as absent. Deleting, as
//! replacing (ADR-150), needs `document:write` over the document's space,
//! at its sensitivity, as well. Downloads and
//! deletions are audited, never with the document's name.

use mimorya_core::authz::{Actor, Sensitivity};
use mimorya_core::documents::limits::MAX_FILE_BYTES;
use mimorya_core::documents::{
    DocumentKind, DocumentSpace, can_write_document, document_read_filter, writable_spaces,
};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::documents::{
    DocumentRow, delete_document, document_bytes, get_visible_document, list_visible_documents,
    read_document_file,
};
use serde_json::json;
use uuid::Uuid;

use super::DocumentService;
use crate::contract::{
    DocumentItem, DocumentList, DocumentQuota, DocumentSpaceOption, stored_text,
};
use crate::services::AccessContext;

/// The documents listed at once, the latest first.
pub const LISTED: usize = 500;

/// A document's file, as deposited.
pub struct DocumentFile {
    pub file_name: String,
    pub media_type: &'static str,
    pub content: Vec<u8>,
}

impl DocumentService {
    /// The documents the person reads, where they may deposit, and the
    /// company's quota if they may.
    pub async fn list(&self, access: &AccessContext) -> anyhow::Result<DocumentList> {
        let filter = document_read_filter(&access.engine, &access.subject, &Actor::user());
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        // One more than listed: whether there are older ones.
        let mut rows = list_visible_documents(&mut tx, &filter, LISTED as i64 + 1).await?;
        let truncated = rows.len() > LISTED;
        rows.truncate(LISTED);
        let spaces = writable_spaces(&access.engine, &access.subject, &Actor::user());
        let quota = if spaces.is_empty() {
            None
        } else {
            Some(DocumentQuota {
                used_bytes: document_bytes(&mut tx).await?,
                quota_bytes: self.quota_bytes,
            })
        };
        tx.commit().await?;
        let documents = rows
            .iter()
            .map(|row| item(access, row))
            .collect::<anyhow::Result<_>>()?;
        let spaces = spaces
            .into_iter()
            .map(|space| DocumentSpaceOption {
                unit_id: space.org_unit_id(),
                name: space_name(access, space),
            })
            .collect();
        Ok(DocumentList {
            documents,
            truncated,
            spaces,
            quota,
            max_file_bytes: MAX_FILE_BYTES as i64,
        })
    }

    /// The document `id`, if the person reads it.
    pub async fn get(
        &self,
        access: &AccessContext,
        id: Uuid,
    ) -> anyhow::Result<Option<DocumentItem>> {
        let filter = document_read_filter(&access.engine, &access.subject, &Actor::user());
        let mut tx = self.db.begin(access.subject.tenant_id).await?;
        let row = get_visible_document(&mut tx, &filter, id).await?;
        tx.commit().await?;
        row.map(|row| item(access, &row)).transpose()
    }

    /// The file of the document `id`, if the person reads it; audited.
    pub async fn file(
        &self,
        access: &AccessContext,
        id: Uuid,
    ) -> anyhow::Result<Option<DocumentFile>> {
        let filter = document_read_filter(&access.engine, &access.subject, &Actor::user());
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(row) = get_visible_document(&mut tx, &filter, id).await? else {
            return Ok(None);
        };
        let Some(content) = read_document_file(&mut tx, id).await? else {
            return Ok(None);
        };
        let entry = AuditEntry::new(tenant_id, "user", "document.downloaded", "success")
            .by_user(access.subject.user_id)
            .resource("document", id)
            .metadata(json!({ "kind": row.kind }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        let kind = DocumentKind::parse(&row.kind)
            .ok_or_else(|| anyhow::anyhow!("stored document kind {} unknown", row.kind))?;
        Ok(Some(DocumentFile {
            file_name: row.file_name,
            media_type: kind.media_type(),
            content,
        }))
    }

    /// Deletes the document `id`, its file and passages, if the person
    /// reads it and may write its space; whether it was. Audited.
    pub async fn delete(&self, access: &AccessContext, id: Uuid) -> anyhow::Result<bool> {
        let filter = document_read_filter(&access.engine, &access.subject, &Actor::user());
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(row) = get_visible_document(&mut tx, &filter, id).await? else {
            return Ok(false);
        };
        if !can_write(access, &row) || !delete_document(&mut tx, id).await? {
            return Ok(false);
        }
        let entry = AuditEntry::new(tenant_id, "user", "document.deleted", "success")
            .by_user(access.subject.user_id)
            .resource("document", id)
            .metadata(json!({ "scope": row.scope, "orgUnitId": row.org_unit_id }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(true)
    }
}

fn space_of(row: &DocumentRow) -> Option<DocumentSpace> {
    DocumentSpace::stored(&row.scope, row.org_unit_id)
}

fn sensitivity_of(row: &DocumentRow) -> Sensitivity {
    // Stored under a check constraint: an unknown level reads as the highest.
    row.sensitivity.parse().unwrap_or(Sensitivity::Restricted)
}

/// The person may replace or delete the document `row`.
pub(super) fn can_write(access: &AccessContext, row: &DocumentRow) -> bool {
    space_of(row).is_some_and(|space| {
        let sensitivity = sensitivity_of(row);
        can_write_document(
            &access.engine,
            &access.subject,
            space,
            sensitivity,
            &Actor::user(),
        )
    })
}

fn space_name(access: &AccessContext, space: DocumentSpace) -> String {
    match space {
        DocumentSpace::Company => access.tenant.name.clone(),
        DocumentSpace::Unit(id) => access
            .org_tree()
            .get(id)
            .map(|unit| unit.name.clone())
            .unwrap_or_default(),
    }
}

fn item(access: &AccessContext, row: &DocumentRow) -> anyhow::Result<DocumentItem> {
    Ok(DocumentItem {
        id: row.id,
        title: row.title.clone(),
        file_name: row.file_name.clone(),
        kind: stored_text("document kind", &row.kind)?,
        scope: stored_text("document scope", &row.scope)?,
        org_unit_id: row.org_unit_id,
        org_unit: row
            .org_unit_id
            .and_then(|id| access.org_tree().get(id))
            .map(|unit| unit.name.clone()),
        sensitivity: sensitivity_of(row).into(),
        byte_size: row.byte_size,
        page_count: row.page_count,
        created_at: crate::http::iso(&row.created_at),
        version: row.version,
        replaced_at: row.replaced_at.as_ref().map(crate::http::iso),
        can_write: can_write(access, row),
    })
}
