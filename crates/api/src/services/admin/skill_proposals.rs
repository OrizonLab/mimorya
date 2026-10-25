//! What the skills' administrators decide of the agent's proposals
//! (ADR-100): one is made active like any version (`skills`), or turned
//! down here. A proposal turned down stays among the skill's versions, as
//! written; an administrator may still make it active later.

use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::skill_proposals::reject_proposal;
use serde_json::json;
use uuid::Uuid;

use super::skills::existing;
use super::{audit_entry, begin_change};
use crate::services::AccessContext;

const PERMISSION: &str = "skill:manage";

/// Turns down the proposal `version` of the skill `id`.
pub async fn reject_skill_proposal(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
    version: i32,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let skill = existing(&mut tx, id).await?;
    if !reject_proposal(&mut tx, id, version).await? {
        return Err(DomainError::validation(format!(
            "La version {version} n’est pas une proposition en attente de décision"
        ))
        .into());
    }
    append_audit(
        &mut tx,
        audit_entry(access, "skill.proposal_rejected", "skill", id).metadata(json!({
            "name": skill.name,
            "version": version,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
