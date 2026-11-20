//! A document's type and confidentiality set by a person (ADR-213, D4): one
//! who reads it and may write it at its level answers for it, or it answers
//! as absent, as for a deletion; the new level must be one they could
//! deposit it at, or it is refused. Set at the version and at the level
//! they were checked at: a replacement or a raise since is not overwritten.
//! What its classification proposed is dropped then, or when they dismiss
//! it. Each audited, never with the document's title.

use mimorya_core::DomainError;
use mimorya_core::authz::{Actor, Sensitivity as CoreSensitivity};
use mimorya_core::documents::{DocumentSpace, DocumentType, can_set_labels, document_read_filter};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::document_labels::{
    LabelProposal, drop_label_proposal, label_proposals, set_document_labels,
};
use mimorya_db::repositories::documents::{DocumentRow, get_visible_document};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::library::can_write;
use super::{DocumentService, stored_sensitivity};
use crate::contract::{
    DocumentLabelProposal, DocumentLabelsUpdate, DocumentProposalDismissal, stored_text,
};
use crate::services::AccessContext;

type Outcome = anyhow::Result<Result<(), DomainError>>;

fn absent() -> DomainError {
    DomainError::not_found("Document introuvable")
}

fn changed() -> DomainError {
    DomainError::conflict(
        "Ce document a changé entre-temps (nouvelle version ou confidentialité relevée) : \
         rechargez-le avant de le modifier",
    )
}

impl DocumentService {
    pub async fn set_labels(
        &self,
        access: &AccessContext,
        id: Uuid,
        update: &DocumentLabelsUpdate,
    ) -> Outcome {
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(row) = writable(&mut tx, access, id).await? else {
            return Ok(Err(absent()));
        };
        let from = stored_sensitivity(&row.sensitivity);
        let to = CoreSensitivity::from(update.sensitivity);
        let space = DocumentSpace::stored(&row.scope, row.org_unit_id);
        let (engine, subject) = (&access.engine, &access.subject);
        if !space
            .is_some_and(|space| can_set_labels(engine, subject, space, from, to, &Actor::user()))
        {
            return Ok(Err(DomainError::forbidden(
                "Vous ne pouvez pas donner ce niveau de confidentialité à ce document",
            )));
        }
        let kind = update.document_type.map(DocumentType::from);
        let stored_kind = kind.map(DocumentType::as_str);
        let version = update.version;
        if row.version != version
            || !set_document_labels(
                &mut tx,
                id,
                version,
                from.as_str(),
                stored_kind,
                to.as_str(),
            )
            .await?
        {
            return Ok(Err(changed()));
        }
        let proposed = drop_label_proposal(&mut tx, id).await?;
        let entry = AuditEntry::new(tenant_id, "user", "document.labels_set", "success")
            .by_user(access.subject.user_id)
            .resource("document", id)
            .metadata(json!({
                "version": version,
                "documentType": { "from": row.document_type, "to": stored_kind },
                "sensitivity": { "from": from.as_str(), "to": to.as_str() },
                "proposed": proposed,
            }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(Ok(()))
    }

    pub async fn dismiss_proposal(
        &self,
        access: &AccessContext,
        id: Uuid,
        dismissal: &DocumentProposalDismissal,
    ) -> Outcome {
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        if writable(&mut tx, access, id).await?.is_none() {
            return Ok(Err(absent()));
        }
        let proposal = label_proposals(&mut tx, &[id]).await?.pop();
        let Some(proposal) = proposal else {
            return Ok(Err(DomainError::not_found(
                "Aucune proposition n’attend pour ce document",
            )));
        };
        if proposal.version != dismissal.version {
            return Ok(Err(changed()));
        }
        drop_label_proposal(&mut tx, id).await?;
        let entry = AuditEntry::new(tenant_id, "user", "document.proposal_dismissed", "success")
            .by_user(access.subject.user_id)
            .resource("document", id)
            .metadata(json!({ "version": proposal.version }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(Ok(()))
    }
}

/// The document `id`, if the person reads it and may write it at its level.
async fn writable(
    conn: &mut PgConnection,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<Option<DocumentRow>> {
    let filter = document_read_filter(&access.engine, &access.subject, &Actor::user());
    let row = get_visible_document(conn, &filter, id).await?;
    Ok(row.filter(|row| can_write(access, row)))
}

/// What a classification left, as its writer reads it.
pub(super) fn proposal_view(proposal: &LabelProposal) -> anyhow::Result<DocumentLabelProposal> {
    Ok(DocumentLabelProposal {
        version: proposal.version,
        document_type: stored("proposed document type", &proposal.document_type)?,
        sensitivity: stored("proposed sensitivity", &proposal.sensitivity)?,
        raised_from: stored("raised sensitivity", &proposal.raised_from)?,
    })
}

fn stored<T: serde::de::DeserializeOwned>(
    column: &str,
    value: &Option<String>,
) -> anyhow::Result<Option<T>> {
    (value.as_deref())
        .map(|value| stored_text(column, value))
        .transpose()
}
