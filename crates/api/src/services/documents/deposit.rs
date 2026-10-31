//! Depositing a document (ADR-095). The space is checked before the file is
//! read: `document:write` there, at or under one's clearance. The file is
//! then read (`reading`); it is recorded with its passages in one
//! transaction, under the company's quota lock, audited without its name
//! nor its text.

use axum::body::Bytes;
use mimorya_core::DomainError;
use mimorya_core::authz::{Actor, Sensitivity};
use mimorya_core::documents::{DocumentSpace, can_write_document};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::documents::{
    NewDocument, insert_document, insert_passages, lock_document_quota,
};
use serde_json::json;

use super::{DocumentService, FileFailure};
use crate::contract::DocumentCreated;
use crate::services::AccessContext;

/// A file to deposit, as the request gave it: its bytes are shared, never
/// copied, up to the store.
pub struct Deposit<'a> {
    pub name: &'a str,
    pub space: DocumentSpace,
    pub sensitivity: Sensitivity,
    pub bytes: Bytes,
}

impl DocumentService {
    /// Refuses a deposit into `space` of a document of `sensitivity` the
    /// person may not make: before its file is read.
    pub fn check_deposit(
        &self,
        access: &AccessContext,
        space: DocumentSpace,
        sensitivity: Sensitivity,
    ) -> Result<(), DomainError> {
        if let DocumentSpace::Unit(id) = space
            && access.org_tree().get(id).is_none()
        {
            return Err(DomainError::not_found("Unité introuvable"));
        }
        let (engine, subject) = (&access.engine, &access.subject);
        if can_write_document(engine, subject, space, sensitivity, &Actor::user()) {
            Ok(())
        } else {
            Err(DomainError::forbidden(
                "Vous ne pouvez pas déposer ce document dans cet espace",
            ))
        }
    }

    pub async fn deposit(
        &self,
        access: &AccessContext,
        deposit: Deposit<'_>,
    ) -> Result<DocumentCreated, FileFailure> {
        self.check_deposit(access, deposit.space, deposit.sensitivity)?;
        let file = (self.read_file(access, deposit.name, &deposit.bytes, None)).await?;
        let tenant_id = access.subject.tenant_id;
        let size = deposit.bytes.len();
        let mut tx = self.db.begin(tenant_id).await?;
        let used = lock_document_quota(&mut tx, tenant_id).await?;
        if used + size as i64 > self.quota_bytes {
            return Err(self.quota_reached().into());
        }
        let id = insert_document(
            &mut tx,
            NewDocument {
                tenant_id,
                scope: deposit.space.scope(),
                org_unit_id: deposit.space.org_unit_id(),
                title: &file.title,
                file_name: &file.name,
                kind: file.kind.as_str(),
                sensitivity: deposit.sensitivity.as_str(),
                page_count: file.page_count,
                uploaded_by_user_id: access.subject.user_id,
                content: &deposit.bytes,
            },
        )
        .await?;
        insert_passages(&mut tx, tenant_id, id, &file.passages).await?;
        let entry = AuditEntry::new(tenant_id, "user", "document.uploaded", "success")
            .by_user(access.subject.user_id)
            .resource("document", id)
            .metadata(json!({
                "scope": deposit.space.scope(),
                "orgUnitId": deposit.space.org_unit_id(),
                "kind": file.kind.as_str(),
                "bytes": size,
                "pages": file.page_count,
                "passages": file.passages.len(),
                "sensitivity": deposit.sensitivity.as_str(),
            }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        // Its passages await their vectors, if the company chose a model.
        self.embeddings.wake();
        Ok(DocumentCreated {
            id,
            page_count: file.page_count,
            passages: file.passages.len() as i64,
        })
    }

    /// The company's documents would hold more than its quota.
    pub(super) fn quota_reached(&self) -> DomainError {
        DomainError::conflict(format!(
            "Le quota de documents de l’entreprise est atteint ({} Mo)",
            self.quota_bytes / (1024 * 1024)
        ))
    }
}
