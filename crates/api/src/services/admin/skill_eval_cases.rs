//! The evaluation cases of a skill (ADR-103), written by an administrator
//! holding `skill:manage` on the whole company: at most
//! `MAX_CASES_PER_SKILL` per skill, each named once in it. A case is
//! changed or deleted through its own identifier; its skill is the one it
//! was written for. The audit names the skill, and the case by its
//! identifier, as a refusal on the case's route does (ADR-038).

use mimorya_core::DomainError;
use mimorya_core::skill_eval_cases::{
    EvalCase, MAX_CASES_PER_SKILL, case_problem, normalized_case,
};
use mimorya_db::Database;
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::skill_eval_cases::{
    EvalCaseRow, EvalCaseText, count_eval_cases, delete_eval_case, eval_case_name_taken,
    get_eval_case, insert_eval_case, update_eval_case,
};
use serde_json::json;
use uuid::Uuid;

use super::skills::existing;
use super::{audit_entry, begin_change};
use crate::services::AccessContext;

const PERMISSION: &str = "skill:manage";

pub struct EvalCaseInput {
    pub name: String,
    pub request: String,
    pub criteria: Vec<String>,
}

/// A new case of the skill `skill_id`; its identifier.
pub async fn create_eval_case(
    db: &Database,
    access: &AccessContext,
    skill_id: Uuid,
    input: EvalCaseInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let case = checked(&input)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let skill = existing(&mut tx, skill_id).await?;
    if count_eval_cases(&mut tx, skill_id).await? >= MAX_CASES_PER_SKILL {
        return Err(DomainError::validation(format!(
            "Au plus {MAX_CASES_PER_SKILL} cas d’évaluation par compétence"
        ))
        .into());
    }
    refuse_taken(&mut tx, skill_id, &case.name, None).await?;
    let id = insert_eval_case(
        &mut tx,
        access.subject.tenant_id,
        skill_id,
        text(&case),
        access.subject.user_id,
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "skill.eval_case_created", "skill", skill_id).metadata(json!({
            "name": skill.name,
            "caseId": id,
            "case": case.name,
            "criteria": case.criteria.len(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// Rewrites the case `id`: its name, request and criteria.
pub async fn change_eval_case(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
    input: EvalCaseInput,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let case = checked(&input)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let current = existing_case(&mut tx, id).await?;
    let skill = existing(&mut tx, current.skill_id).await?;
    refuse_taken(&mut tx, current.skill_id, &case.name, Some(id)).await?;
    update_eval_case(&mut tx, id, text(&case)).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "skill.eval_case_updated", "skill", current.skill_id).metadata(json!({
            "name": skill.name,
            "caseId": id,
            "case": case.name,
            "previousCase": current.name,
            "criteria": case.criteria.len(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn remove_eval_case(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let current = existing_case(&mut tx, id).await?;
    let skill = existing(&mut tx, current.skill_id).await?;
    delete_eval_case(&mut tx, id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "skill.eval_case_deleted", "skill", current.skill_id)
            .metadata(json!({ "name": skill.name, "caseId": id, "case": current.name })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The case as stored, once it passed the rules.
fn checked(input: &EvalCaseInput) -> Result<EvalCase, DomainError> {
    let case = normalized_case(&input.name, &input.request, &input.criteria);
    match case_problem(&case) {
        Some(problem) => Err(DomainError::validation(problem)),
        None => Ok(case),
    }
}

fn text(case: &EvalCase) -> EvalCaseText<'_> {
    EvalCaseText {
        name: &case.name,
        request: &case.request,
        criteria: &case.criteria,
    }
}

async fn existing_case(tx: &mut TenantTx, id: Uuid) -> anyhow::Result<EvalCaseRow> {
    get_eval_case(tx, id)
        .await?
        .ok_or_else(|| DomainError::not_found("Cas d’évaluation introuvable").into())
}

async fn refuse_taken(
    tx: &mut TenantTx,
    skill_id: Uuid,
    name: &str,
    except: Option<Uuid>,
) -> anyhow::Result<()> {
    if eval_case_name_taken(tx, skill_id, name, except).await? {
        return Err(DomainError::conflict(
            "Un cas d’évaluation de cette compétence porte déjà ce nom",
        )
        .into());
    }
    Ok(())
}
