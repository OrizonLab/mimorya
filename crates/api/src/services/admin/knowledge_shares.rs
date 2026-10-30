//! Knowledge shared between units (ADR-143): an administrator of the
//! company's access policies (`policy:manage`, company-wide) shares a
//! unit's knowledge (the memory filed in its space, the documents deposited
//! there, either or both) with another unit, and if they say so the units
//! under it, up to a sensitivity, until a date if they set one; or removes
//! a share. What a share may say is `core`'s (`share_problem`): never one
//! its author would read through, which takes another administrator
//! (ADR-019, ADR-022). A unit is shared once with another. Unknown units are
//! refused and traced (ADR-054). Each change is audited and applies at once
//! (the company's authorization reloaded, in every process).

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_core::knowledge_shares::{KnowledgeShare, ShareProblem, share_problem};
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::knowledge_shares::{
    NewKnowledgeShare, create_knowledge_share, delete_knowledge_share, knowledge_share_exists,
};
use mimorya_db::repositories::org::load_user_access;
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change, current_tree};
use crate::services::{AccessContext, AccessService};

const PERMISSION: &str = "policy:manage";

/// Shares a unit's knowledge as `share` says; the share's identifier.
pub async fn share_knowledge(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    share: KnowledgeShare,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let (source, target) = (share.source_unit_id, share.target_unit_id);
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let tree = current_tree(&mut tx).await?;
    // The author's units as stored now, not as their session saw them.
    let author = load_user_access(&mut tx, access.subject.user_id).await?;
    let author_units = author.map(|a| a.subject.org_unit_ids).unwrap_or_default();
    match share_problem(&share, &tree, (&author_units, Utc::now())) {
        None => {}
        Some(ShareProblem::Invalid(message)) => return Err(DomainError::validation(message).into()),
        Some(ShareProblem::UnknownUnit(unit)) => {
            return Err(DomainError::unknown("Unité inconnue", "org_unit", unit).into());
        }
        Some(ShareProblem::AuthorReads) => {
            return Err(DomainError::forbidden(
                "Vous seriez parmi les lecteurs de ce partage : un autre administrateur doit le faire",
            )
            .into());
        }
    }
    if knowledge_share_exists(&mut tx, (source, target)).await? {
        return Err(DomainError::conflict(
            "Les connaissances de cette unité sont déjà partagées avec celle-ci",
        )
        .into());
    }
    let said = json!({
        "sourceOrgUnitId": source,
        "targetOrgUnitId": target,
        "includeDescendants": share.include_descendants,
        "memory": share.memory,
        "documents": share.documents,
        "maxSensitivity": share.ceiling.as_str(),
        "expiresAt": share.expires_at,
    });
    let created_by_user_id = access.subject.user_id;
    let new = NewKnowledgeShare {
        tenant_id,
        share,
        created_by_user_id,
    };
    let id = create_knowledge_share(&mut tx, new).await?;
    let entry =
        audit_entry(access, "knowledge_share.created", "knowledge_share", id).metadata(said);
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    access_service.invalidate(tenant_id);
    Ok(id)
}

/// Removes the share `id`: its members read the unit's knowledge no more.
pub async fn remove_knowledge_share(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let Some(removed) = delete_knowledge_share(&mut tx, id).await? else {
        return Err(DomainError::not_found("Partage introuvable").into());
    };
    let entry =
        audit_entry(access, "knowledge_share.deleted", "knowledge_share", id).metadata(json!({
            "sourceOrgUnitId": removed.source_org_unit_id,
            "targetOrgUnitId": removed.target_org_unit_id,
        }));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    access_service.invalidate(tenant_id);
    Ok(())
}
