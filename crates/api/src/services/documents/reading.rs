//! A file read before it is kept, at a deposit or a replacement by a new
//! version (ADR-095, ADR-150): named, recognised by its first bytes, read
//! by the extraction process and cut into passages. A file the process
//! could not read (it timed out or failed) is refused as unreadable, and
//! that failure audited, with the document a replacement was for, never
//! with the file's name nor its text.

use axum::body::Bytes;
use mimorya_core::DomainError;
use mimorya_core::documents::{DocumentKind, Passage, Refusal, file_name, passages, title_of};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;

use uuid::Uuid;

use super::{DocumentService, ExtractionFailure};
use crate::services::AccessContext;

/// Why a deposit or a replacement failed: the file, as the reader found
/// it; a refusal of the request; or an error of the service.
#[derive(Debug)]
pub enum FileFailure {
    Refused(Refusal),
    Domain(DomainError),
    Internal(anyhow::Error),
}

impl From<DomainError> for FileFailure {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<anyhow::Error> for FileFailure {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

impl From<sqlx::Error> for FileFailure {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

/// A file read: its name as kept, its title, its kind, its passages and,
/// for a paged file, its pages.
pub(super) struct ReadFile {
    pub name: String,
    pub title: String,
    pub kind: DocumentKind,
    pub passages: Vec<Passage>,
    pub page_count: Option<i32>,
}

impl DocumentService {
    /// Reads the file `bytes`, named `raw_name` by the request: a new
    /// document's, or a new version of the document `replacing`.
    pub(super) async fn read_file(
        &self,
        access: &AccessContext,
        raw_name: &str,
        bytes: &Bytes,
        replacing: Option<Uuid>,
    ) -> Result<ReadFile, FileFailure> {
        let name = file_name(raw_name)
            .ok_or_else(|| DomainError::validation("Nom de fichier invalide"))?;
        let kind = DocumentKind::recognise(&name, bytes).map_err(FileFailure::Refused)?;
        let extracted = match self.extractor.extract(kind, bytes.clone()).await {
            Ok(extracted) => extracted,
            Err(ExtractionFailure::Refused(refusal)) => {
                return Err(FileFailure::Refused(refusal));
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
                self.audit_rejected(access, kind, bytes.len(), &failure, replacing)
                    .await?;
                return Err(FileFailure::Refused(Refusal::Unreadable));
            }
        };
        let page_count = extracted
            .paged
            .then(|| i32::try_from(extracted.pages.len()).unwrap_or(i32::MAX));
        Ok(ReadFile {
            title: title_of(&name),
            passages: passages(&extracted),
            name,
            kind,
            page_count,
        })
    }

    /// Audits a file the extraction process could not read: its kind and
    /// size, why, and the document it was to replace; never its name nor
    /// its content.
    async fn audit_rejected(
        &self,
        access: &AccessContext,
        kind: DocumentKind,
        size: usize,
        failure: &ExtractionFailure,
        replacing: Option<Uuid>,
    ) -> anyhow::Result<()> {
        let reason = match failure {
            ExtractionFailure::TimedOut => "timed_out",
            _ => "reader_failed",
        };
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        let mut entry = AuditEntry::new(tenant_id, "user", "document.rejected", "failure")
            .by_user(access.subject.user_id)
            .reason(reason)
            .metadata(json!({ "kind": kind.as_str(), "bytes": size }));
        if let Some(id) = replacing {
            entry = entry.resource("document", id);
        }
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(())
    }
}
