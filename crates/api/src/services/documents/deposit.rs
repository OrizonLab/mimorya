//! Depositing a document (ADR-095). The space is checked before the file is
//! read: `document:write` there, at or under one's clearance. The file is
//! then named, recognised by its first bytes, read by the extraction
//! process and cut into passages; it is recorded with them in one
//! transaction, under the company's quota lock, audited without its name
//! nor its text. A file the process could not read (it timed out or
//! failed) is refused as unreadable, and that failure audited.

use axum::body::Bytes;
use mimorya_core::DomainError;
use mimorya_core::authz::{Actor, Sensitivity};
use mimorya_core::documents::{
    DocumentKind, DocumentSpace, Refusal, can_write_document, file_name, passages, title_of,
};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::documents::{
    NewDocument, insert_document, insert_passages, lock_document_quota,
};
use serde_json::json;

use super::{DocumentService, ExtractionFailure};
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

/// Why a deposit failed: the file, as the reader found it; a refusal of
/// the request; or an error of the service.
#[derive(Debug)]
pub enum DepositFailure {
    Refused(Refusal),
    Domain(DomainError),
    Internal(anyhow::Error),
}

impl From<DomainError> for DepositFailure {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<anyhow::Error> for DepositFailure {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

impl From<sqlx::Error> for DepositFailure {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
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
    ) -> Result<DocumentCreated, DepositFailure> {
        self.check_deposit(access, deposit.space, deposit.sensitivity)?;
        let name = file_name(deposit.name)
            .ok_or_else(|| DomainError::validation("Nom de fichier invalide"))?;
        let kind =
            DocumentKind::recognise(&name, &deposit.bytes).map_err(DepositFailure::Refused)?;
        let tenant_id = access.subject.tenant_id;
        let size = deposit.bytes.len();
        let extracted = match self.extractor.extract(kind, deposit.bytes.clone()).await {
            Ok(extracted) => extracted,
            Err(ExtractionFailure::Refused(refusal)) => {
                return Err(DepositFailure::Refused(refusal));
            }
            Err(ExtractionFailure::Busy) => {
                return Err(DomainError::unavailable(
                    "Trop de documents en cours de lecture : réessayez dans un instant",
                )
                .into());
            }
            Err(ExtractionFailure::NotStarted(error)) => {
                return Err(anyhow::anyhow!("document reader not started: {error}").into());
            }
            Err(failure @ (ExtractionFailure::TimedOut | ExtractionFailure::Failed(_))) => {
                self.audit_rejected(access, kind, size, &failure).await?;
                return Err(DepositFailure::Refused(Refusal::Unreadable));
            }
        };
        let cut = passages(&extracted);
        let page_count = extracted
            .paged
            .then(|| i32::try_from(extracted.pages.len()).unwrap_or(i32::MAX));
        let mut tx = self.db.begin(tenant_id).await?;
        let used = lock_document_quota(&mut tx, tenant_id).await?;
        if used + size as i64 > self.quota_bytes {
            return Err(DomainError::conflict(format!(
                "Le quota de documents de l’entreprise est atteint ({} Mo)",
                self.quota_bytes / (1024 * 1024)
            ))
            .into());
        }
        let title = title_of(&name);
        let id = insert_document(
            &mut tx,
            NewDocument {
                tenant_id,
                scope: deposit.space.scope(),
                org_unit_id: deposit.space.org_unit_id(),
                title: &title,
                file_name: &name,
                kind: kind.as_str(),
                sensitivity: deposit.sensitivity.as_str(),
                page_count,
                uploaded_by_user_id: access.subject.user_id,
                content: &deposit.bytes,
            },
        )
        .await?;
        insert_passages(&mut tx, tenant_id, id, &cut).await?;
        let entry = AuditEntry::new(tenant_id, "user", "document.uploaded", "success")
            .by_user(access.subject.user_id)
            .resource("document", id)
            .metadata(json!({
                "scope": deposit.space.scope(),
                "orgUnitId": deposit.space.org_unit_id(),
                "kind": kind.as_str(),
                "bytes": size,
                "pages": page_count,
                "passages": cut.len(),
                "sensitivity": deposit.sensitivity.as_str(),
            }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(DocumentCreated {
            id,
            page_count,
            passages: cut.len() as i64,
        })
    }

    /// Audits a file the extraction process could not read: its kind and
    /// size, why, never its name nor its content.
    async fn audit_rejected(
        &self,
        access: &AccessContext,
        kind: DocumentKind,
        size: usize,
        failure: &ExtractionFailure,
    ) -> anyhow::Result<()> {
        let reason = match failure {
            ExtractionFailure::TimedOut => "timed_out",
            _ => "reader_failed",
        };
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        let entry = AuditEntry::new(tenant_id, "user", "document.rejected", "failure")
            .by_user(access.subject.user_id)
            .reason(reason)
            .metadata(json!({ "kind": kind.as_str(), "bytes": size }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(())
    }
}
