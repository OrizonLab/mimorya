//! The company's skills (ADR-052): an administrator holding `skill:manage` on
//! the whole company writes a skill and its versions, makes one of them
//! active (an older one included: that is the way back), retires the skill,
//! and deletes one that is not active. A version's content never changes
//! once written. Runs read the active versions when they start: there is no
//! cache to refresh.

use mimorya_core::DomainError;
use mimorya_core::skill_rules::{
    MAX_ACTIVE_SKILLS, content_problem, line_problem, name_problem, normalized_content,
};
use mimorya_db::Database;
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::skills::{
    self, NewSkill, NewSkillVersion, SkillRow, activate_skill_version, count_active_skills,
    get_skill, insert_skill, insert_skill_version, skill_name_taken, update_skill_description,
};
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change};
use crate::services::AccessContext;

const PERMISSION: &str = "skill:manage";

pub struct SkillInput {
    pub name: String,
    pub description: String,
    pub content: String,
    pub changelog: String,
}

/// A skill and its first version, both drafts: nothing reaches the agent
/// before a version is made active.
pub async fn create_skill(
    db: &Database,
    access: &AccessContext,
    input: SkillInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    refuse(name_problem(&input.name))?;
    refuse(line_problem("Description", &input.description))?;
    let content = checked_content(&input.content, &input.changelog)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    if skill_name_taken(&mut tx, &input.name).await? {
        return Err(DomainError::conflict("Une compétence porte déjà ce nom").into());
    }
    let id = insert_skill(
        &mut tx,
        NewSkill {
            tenant_id: access.subject.tenant_id,
            name: &input.name,
            description: &input.description,
            created_by: access.subject.user_id,
        },
    )
    .await?;
    let version = add_version(&mut tx, access, id, &content, &input.changelog).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "skill.created", "skill", id).metadata(json!({
            "name": input.name,
            "description": input.description,
            "version": version,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn update_skill(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
    description: String,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    refuse(line_problem("Description", &description))?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let skill = existing(&mut tx, id).await?;
    update_skill_description(&mut tx, id, &description).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "skill.updated", "skill", id).metadata(json!({
            "name": skill.name,
            "description": description,
            "previousDescription": skill.description,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// A new draft version; its number.
pub async fn add_skill_version(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
    content: String,
    changelog: String,
) -> anyhow::Result<i32> {
    access.require_tenant_wide(PERMISSION)?;
    let content = checked_content(&content, &changelog)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let skill = existing(&mut tx, id).await?;
    let version = add_version(&mut tx, access, id, &content, &changelog).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "skill.version_created", "skill", id).metadata(json!({
            "name": skill.name,
            "version": version,
            "changelog": changelog,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(version)
}

/// Makes `version` the one runs load, the version active before retired.
pub async fn activate_skill(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
    version: i32,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let skill = existing(&mut tx, id).await?;
    if skill.active_version == Some(version) {
        return Err(DomainError::validation("Cette version est déjà active").into());
    }
    if skill.status != "active" && count_active_skills(&mut tx).await? >= MAX_ACTIVE_SKILLS {
        return Err(DomainError::validation(format!(
            "Au plus {MAX_ACTIVE_SKILLS} compétences actives : retirez-en une d’abord"
        ))
        .into());
    }
    let user_id = access.subject.user_id;
    if !activate_skill_version(&mut tx, id, version, user_id).await? {
        return Err(DomainError::validation(format!(
            "Cette compétence n’a pas de version {version}"
        ))
        .into());
    }
    append_audit(
        &mut tx,
        audit_entry(access, "skill.activated", "skill", id).metadata(json!({
            "name": skill.name,
            "version": version,
            "previousVersion": skill.active_version,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// No version is active any more: runs no longer see the skill.
pub async fn retire_skill(db: &Database, access: &AccessContext, id: Uuid) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let skill = existing(&mut tx, id).await?;
    if skill.status != "active" {
        return Err(DomainError::validation("Cette compétence n’est pas active").into());
    }
    skills::retire_skill(&mut tx, id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "skill.retired", "skill", id).metadata(json!({
            "name": skill.name,
            "version": skill.active_version,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The skill and its versions; an active skill is retired first.
pub async fn delete_skill(db: &Database, access: &AccessContext, id: Uuid) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let skill = existing(&mut tx, id).await?;
    if skill.status == "active" {
        return Err(
            DomainError::conflict("Compétence active : retirez-la avant de la supprimer").into(),
        );
    }
    skills::delete_skill(&mut tx, id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "skill.deleted", "skill", id).metadata(json!({
            "name": skill.name,
            "versions": skill.latest_version,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn existing(tx: &mut TenantTx, id: Uuid) -> anyhow::Result<SkillRow> {
    get_skill(tx, id)
        .await?
        .ok_or_else(|| DomainError::not_found("Compétence introuvable").into())
}

async fn add_version(
    tx: &mut TenantTx,
    access: &AccessContext,
    skill_id: Uuid,
    content: &str,
    changelog: &str,
) -> anyhow::Result<i32> {
    let version = NewSkillVersion {
        tenant_id: access.subject.tenant_id,
        skill_id,
        content,
        changelog,
        created_by: access.subject.user_id,
        proposed_by_run: None,
    };
    Ok(insert_skill_version(tx, version).await?)
}

/// The content as stored, once it and the changelog passed the checks.
fn checked_content(content: &str, changelog: &str) -> Result<String, DomainError> {
    refuse(line_problem("Note de version", changelog))?;
    let content = normalized_content(content);
    refuse(content_problem(&content))?;
    Ok(content)
}

fn refuse(problem: Option<String>) -> Result<(), DomainError> {
    problem.map_or(Ok(()), |problem| Err(DomainError::validation(problem)))
}
