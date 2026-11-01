//! An evaluation asked for by an administrator holding `skill:manage` on
//! the whole company (ADR-104): a version of a skill tried on its cases,
//! as they stand now, beside the active version when another one is
//! evaluated. Every case runs for the administrator, with their rights: they
//! must be able to use the agent, and the agent to read the skills for
//! them; the company needs a model. One evaluation queued or running per
//! company, none once its evaluations reached what they may cost over
//! thirty days (ADR-154); the oldest of a skill are deleted past the last
//! ones kept. A process that executes runs plays it
//! (`services::skill_evaluations`).

use mimorya_core::DomainError;
use mimorya_core::authz::Actor;
use mimorya_core::skill_evaluations::EVALUATIONS_KEPT_PER_SKILL;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::skill_eval_cases::list_eval_cases;
use mimorya_db::repositories::skill_evaluations::{
    NewEvaluation, evaluation_in_progress, insert_evaluation, prune_evaluations, version_text,
};
use serde_json::json;
use uuid::Uuid;

use super::skills::existing;
use super::{audit_entry, begin_change};
use crate::app::AppState;
use crate::services::AccessContext;
use crate::services::skill_evaluations::{StoredCase, spending, wake};

const PERMISSION: &str = "skill:manage";

/// Queues the evaluation of `version` of the skill `skill_id`; its id.
pub async fn launch_evaluation(
    services: &AppState,
    access: &AccessContext,
    skill_id: Uuid,
    version: i32,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    if !access.has("agent:use") {
        return Err(DomainError::forbidden(
            "Évaluer une compétence demande de pouvoir utiliser l’assistant : chaque cas est \
             joué pour vous",
        )
        .into());
    }
    if !access
        .engine
        .has(&access.subject, "skill:read", &Actor::agent(None))
    {
        return Err(DomainError::validation(
            "L’assistant ne lit pas les compétences pour vous : il ne chargerait pas celle évaluée",
        )
        .into());
    }
    let tenant_id = access.subject.tenant_id;
    if services.llm.provider_for(tenant_id).await?.is_none() {
        return Err(
            DomainError::validation("Aucun modèle n’est configuré pour l’entreprise").into(),
        );
    }
    let mut tx = begin_change(&services.db, access, PERMISSION).await?;
    let skill = existing(&mut tx, skill_id).await?;
    if version_text(&mut tx, skill_id, version).await?.is_none() {
        return Err(DomainError::validation(format!(
            "Cette compétence n’a pas de version {version}"
        ))
        .into());
    }
    let cases: Vec<StoredCase> = list_eval_cases(&mut tx, skill_id)
        .await?
        .into_iter()
        .map(|c| StoredCase {
            name: c.name,
            request: c.request,
            criteria: c.criteria,
        })
        .collect();
    if cases.is_empty() {
        return Err(DomainError::validation(
            "Écrivez d’abord au moins un cas d’évaluation de cette compétence",
        )
        .into());
    }
    if evaluation_in_progress(&mut tx).await? {
        return Err(DomainError::conflict(
            "Une évaluation est déjà en cours dans l’entreprise : attendez sa fin",
        )
        .into());
    }
    let spending = spending(&mut tx).await?;
    if spending.reached() {
        return Err(DomainError::conflict(spending.refusal()).into());
    }
    let reference_version = skill.active_version.filter(|active| *active != version);
    prune_evaluations(&mut tx, skill_id, EVALUATIONS_KEPT_PER_SKILL - 1).await?;
    let snapshot = serde_json::to_value(&cases)?;
    let id = insert_evaluation(
        &mut tx,
        NewEvaluation {
            tenant_id,
            skill_id,
            version,
            reference_version,
            launched_by: access.subject.user_id,
            cases: &snapshot,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "skill.evaluation_requested", "skill", skill_id).metadata(json!({
            "name": skill.name,
            "evaluationId": id,
            "version": version,
            "referenceVersion": reference_version,
            "cases": cases.len(),
        })),
    )
    .await?;
    tx.commit().await?;
    wake(services);
    Ok(id)
}
