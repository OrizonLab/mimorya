//! Knowledge shared between units (ADR-143): an administrator of the
//! company's access policies (`policy:manage`, company-wide) shares a
//! unit's knowledge (the memory filed in its space, the documents deposited
//! there, either or both) with another unit, and if they say so the units
//! under it, up to a sensitivity, until a date if they set one; or removes
//! a share. A unit is shared once with another; its own members, and those
//! of the units under it, read its knowledge already. Unknown units are
//! refused and traced (ADR-054). Each change is audited and applies at once
//! (the company's authorization reloaded, in every process).

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_core::authz::Sensitivity;
use mimorya_core::knowledge_shares::KnowledgeShare;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::knowledge_shares::{
    NewKnowledgeShare, create_knowledge_share, delete_knowledge_share, knowledge_share_exists,
};
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change, current_tree};
use crate::services::{AccessContext, AccessService};

const PERMISSION: &str = "policy:manage";

/// A share to make.
pub struct ShareInput {
    pub source_org_unit_id: Uuid,
    pub target_org_unit_id: Uuid,
    pub include_descendants: bool,
    pub memory: bool,
    pub documents: bool,
    pub max_sensitivity: Sensitivity,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Shares a unit's knowledge as `input` says; the share's identifier.
pub async fn share_knowledge(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    input: ShareInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let (source, target) = (input.source_org_unit_id, input.target_org_unit_id);
    if !input.memory && !input.documents {
        return Err(
            DomainError::validation("Partagez la mémoire, les documents, ou les deux").into(),
        );
    }
    if source == target {
        return Err(
            DomainError::validation("Une unité partage ses connaissances avec une autre").into(),
        );
    }
    if input.expires_at.is_some_and(|at| at <= Utc::now()) {
        return Err(DomainError::validation("La date d’échéance doit être dans le futur").into());
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let tree = current_tree(&mut tx).await?;
    for unit in [source, target] {
        if tree.get(unit).is_none() {
            return Err(DomainError::unknown("Unité inconnue", "org_unit", unit).into());
        }
    }
    // Its members, and those of the units under it, read it already.
    if tree.ancestors(target).iter().any(|a| a.id == source) {
        return Err(DomainError::validation(
            "Les membres de cette unité lisent déjà les connaissances de l’unité qui la contient",
        )
        .into());
    }
    if knowledge_share_exists(&mut tx, (source, target)).await? {
        return Err(DomainError::conflict(
            "Les connaissances de cette unité sont déjà partagées avec celle-ci",
        )
        .into());
    }
    let share = KnowledgeShare {
        id: Uuid::nil(),
        source_unit_id: source,
        target_unit_id: target,
        include_descendants: input.include_descendants,
        memory: input.memory,
        documents: input.documents,
        ceiling: input.max_sensitivity,
        expires_at: input.expires_at,
    };
    let created_by_user_id = access.subject.user_id;
    let new = NewKnowledgeShare {
        tenant_id,
        share,
        created_by_user_id,
    };
    let id = create_knowledge_share(&mut tx, new).await?;
    let entry =
        audit_entry(access, "knowledge_share.created", "knowledge_share", id).metadata(json!({
            "sourceOrgUnitId": source,
            "targetOrgUnitId": target,
            "includeDescendants": input.include_descendants,
            "memory": input.memory,
            "documents": input.documents,
            "maxSensitivity": input.max_sensitivity.as_str(),
            "expiresAt": input.expires_at,
        }));
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
