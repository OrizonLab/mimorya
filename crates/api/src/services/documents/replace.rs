//! A document replaced by a new version of its file (ADR-150). The person
//! must read it and may write its space at its sensitivity, or it answers
//! as absent, as for a deletion: checked before the file is read, and again
//! in the transaction that keeps it. The replacement names the version it
//! replaces: one made since is not overwritten. The file is read as at a
//! deposit; the document keeps its identifier, space, sensitivity and
//! first deposit, the company's quota counting only what the new file adds.
//! Audited without its name nor its text, as a file the reader could not
//! read is, with the document it was for; its new passages await their
//! vectors.

use axum::body::Bytes;
use mimorya_core::DomainError;
use mimorya_core::authz::Actor;
use mimorya_core::documents::document_read_filter;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::document_versions::{NewVersion, replace_document_file};
use mimorya_db::repositories::documents::{DocumentRow, get_visible_document, lock_document_quota};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::library::can_write;
use super::{DocumentService, FileFailure};
use crate::contract::DocumentReplaced;
use crate::services::AccessContext;

/// A new version of a document's file, as the request gave it.
pub struct Replacement<'a> {
    pub document_id: Uuid,
    pub name: &'a str,
    /// The version it replaces, the one the person last saw.
    pub replaces: i32,
    pub bytes: Bytes,
}

fn absent() -> DomainError {
    DomainError::not_found("Document introuvable")
}

impl DocumentService {
    /// Refuses, before its file is read, a replacement of the document `id`
    /// the person may not make, or of a version replaced since.
    pub async fn check_replacement(
        &self,
        access: &AccessContext,
        id: Uuid,
        replaces: i32,
    ) -> Result<(), FileFailure> {
        let mut tx = self.db.begin(access.subject.tenant_id).await?;
        replaceable(&mut tx, access, id, replaces).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn replace(
        &self,
        access: &AccessContext,
        replacement: Replacement<'_>,
    ) -> Result<DocumentReplaced, FileFailure> {
        let (id, replaces) = (replacement.document_id, replacement.replaces);
        self.check_replacement(access, id, replaces).await?;
        let bytes = &replacement.bytes;
        let file = (self.read_file(access, replacement.name, bytes, Some(id))).await?;
        let tenant_id = access.subject.tenant_id;
        let size = bytes.len() as i64;
        let mut tx = self.db.begin(tenant_id).await?;
        // Replacements and deposits wait for one another here: the
        // document read next is the one replaced.
        let used = lock_document_quota(&mut tx, tenant_id).await?;
        let current = replaceable(&mut tx, access, id, replaces).await?;
        let former = i64::from(current.byte_size);
        // A smaller file is always taken, even over a quota lowered since.
        if size > former && used - former + size > self.quota_bytes {
            return Err(self.quota_reached().into());
        }
        let version = NewVersion {
            tenant_id,
            document_id: id,
            replaces,
            title: &file.title,
            file_name: &file.name,
            kind: file.kind.as_str(),
            page_count: file.page_count,
            replaced_by_user_id: access.subject.user_id,
            content: bytes,
            passages: &file.passages,
        };
        // Only a deletion, which takes no lock, can come in between.
        let version = (replace_document_file(&mut tx, version).await?).ok_or_else(absent)?;
        let entry = AuditEntry::new(tenant_id, "user", "document.replaced", "success")
            .by_user(access.subject.user_id)
            .resource("document", id)
            .metadata(json!({
                "version": version,
                "kind": file.kind.as_str(),
                "bytes": size,
                "pages": file.page_count,
                "passages": file.passages.len(),
            }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        // Its new passages await their vectors, if the company chose a model.
        self.embeddings.wake();
        Ok(DocumentReplaced {
            version,
            page_count: file.page_count,
            passages: file.passages.len() as i64,
        })
    }
}

/// The document `id`, if the person reads it, may write it and it is still
/// at the version `replaces`.
async fn replaceable(
    conn: &mut PgConnection,
    access: &AccessContext,
    id: Uuid,
    replaces: i32,
) -> Result<DocumentRow, FileFailure> {
    let filter = document_read_filter(&access.engine, &access.subject, &Actor::user());
    let row = (get_visible_document(conn, &filter, id).await?)
        .filter(|row| can_write(access, row))
        .ok_or_else(absent)?;
    if row.version != replaces {
        return Err(DomainError::conflict(
            "Ce document a été remplacé entre-temps : rechargez la liste pour voir sa dernière version",
        )
        .into());
    }
    Ok(row)
}
