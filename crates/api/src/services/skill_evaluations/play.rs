//! One evaluation played (ADR-104): claimed, then each case in turn, for
//! the version evaluated and, beside it, for the active one, each result
//! written as it comes while the evaluation runs: one failed meanwhile as
//! interrupted (the lock's session lost, another pass took over) is played
//! no further. Before each case, whom it runs for must still
//! administer the company's skills and use the agent: otherwise the
//! evaluation ends there, failed, with the results it has. Its end is
//! audited with what its cases add up to, never an answer.

use std::sync::Arc;

use mimorya_agent::llm::LlmProvider;
use mimorya_core::skill_evaluations::summarize;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::skill_evaluations::{
    EvaluationRow, NewResult, VersionText, claim_evaluation, finish_evaluation, get_evaluation,
    record_result, results_of, version_text,
};
use serde_json::json;
use uuid::Uuid;

use super::case::{CaseRequest, Played, StoredCase, play_case};
use super::ports::{CaseScope, EvaluatedVersion};
use super::view::case_results;
use crate::app::Services;
use crate::services::AccessContext;
use crate::services::runs::stopped_by;

/// Why an evaluation fails, as its administrators read it.
const INTERRUPTED: &str = "Évaluation interrompue par un arrêt du service : relancez-la.";
const INTERNAL: &str = "Erreur interne : l’évaluation n’a pas pu se terminer.";
const NO_MODEL: &str = "Aucun modèle n’est configuré pour l’entreprise.";
const LAUNCHER_GONE: &str = "Le compte qui l’a demandée n’existe plus.";
const NOT_ADMIN: &str = "Qui l’a demandée n’administre plus les compétences de toute l’entreprise.";
const VERSION_GONE: &str = "La version évaluée n’existe plus.";

/// Plays the queued evaluation `id`; nothing when it is not queued any more.
pub async fn play(services: &Services, tenant_id: Uuid, id: Uuid) -> anyhow::Result<()> {
    let mut tx = services.db.begin(tenant_id).await?;
    let claimed = claim_evaluation(&mut tx, id).await?;
    tx.commit().await?;
    let Some(evaluation) = claimed else {
        return Ok(());
    };
    let failure = match play_cases(services, tenant_id, &evaluation).await {
        Ok(failure) => failure,
        Err(error) => {
            tracing::warn!(evaluation_id = %id, error = format!("{error:#}"), "Evaluation failed");
            Some(INTERNAL.to_string())
        }
    };
    finish(services, tenant_id, id, failure.as_deref()).await
}

/// Fails the evaluation `id`, which a stopped process left running.
pub async fn fail_interrupted(
    services: &Services,
    tenant_id: Uuid,
    id: Uuid,
) -> anyhow::Result<()> {
    finish(services, tenant_id, id, Some(INTERRUPTED)).await
}

async fn play_cases(
    services: &Services,
    tenant_id: Uuid,
    evaluation: &EvaluationRow,
) -> anyhow::Result<Option<String>> {
    let Some(launcher) = evaluation.launched_by_user_id else {
        return Ok(Some(LAUNCHER_GONE.into()));
    };
    let cases: Vec<StoredCase> = serde_json::from_value(evaluation.cases.clone())?;
    let Some(provider) = services.llm.provider_for(tenant_id).await? else {
        return Ok(Some(NO_MODEL.into()));
    };
    let mut tx = services.db.begin(tenant_id).await?;
    let skill_id = evaluation.skill_id;
    let evaluated = version_text(&mut tx, skill_id, evaluation.version).await?;
    let reference = match evaluation.reference_version {
        Some(version) => version_text(&mut tx, skill_id, version)
            .await?
            .map(|text| played_as(text, version)),
        None => None,
    };
    tx.commit().await?;
    let Some(evaluated) = evaluated else {
        return Ok(Some(VERSION_GONE.into()));
    };
    let mut versions = vec![(false, played_as(evaluated, evaluation.version))];
    versions.extend(reference.map(|version| (true, version)));
    let scope = CaseScope {
        tenant_id,
        evaluation_id: evaluation.id,
        user_id: launcher,
    };
    for (position, case) in cases.iter().enumerate() {
        for (reference, version) in &versions {
            let access = match launcher_may(services, scope).await? {
                Ok(access) => access,
                Err(why) => return Ok(Some(why)),
            };
            let played = play_case(
                services,
                CaseRequest {
                    scope,
                    access: &access,
                    provider: Arc::<dyn LlmProvider>::clone(&provider),
                    version: version.clone(),
                    case,
                },
            )
            .await;
            let position = i32::try_from(position)?;
            if !record(services, scope, position, *reference, &played).await? {
                // Failed as interrupted meanwhile, by a pass that took over
                // the lock (its session lost): it is played no further.
                return Ok(Some(INTERRUPTED.into()));
            }
        }
    }
    Ok(None)
}

fn played_as(text: VersionText, version: i32) -> EvaluatedVersion {
    EvaluatedVersion {
        name: text.name,
        description: text.description,
        version,
        content: text.content,
    }
}

/// The access of whom the evaluation runs for, or why it may not go on.
async fn launcher_may(
    services: &Services,
    scope: CaseScope,
) -> anyhow::Result<Result<AccessContext, String>> {
    let context = services
        .access
        .active_context(scope.tenant_id, scope.user_id)
        .await?;
    if let Some(why) = stopped_by(context.as_ref()) {
        return Ok(Err(why.into()));
    }
    Ok(match context {
        Some(context) if context.require_tenant_wide("skill:manage").is_ok() => Ok(context),
        _ => Err(NOT_ADMIN.into()),
    })
}

/// Writes the case's result; `false` when the evaluation is not running
/// any more.
async fn record(
    services: &Services,
    scope: CaseScope,
    position: i32,
    reference: bool,
    played: &Played,
) -> anyhow::Result<bool> {
    let tools = serde_json::to_value(&played.tools)?;
    let mut tx = services.db.begin(scope.tenant_id).await?;
    let written = record_result(
        &mut tx,
        NewResult {
            tenant_id: scope.tenant_id,
            evaluation_id: scope.evaluation_id,
            position,
            reference,
            outcome: played.outcome.as_str(),
            issues: &played.issues,
            answer: played.answer.as_deref(),
            error: played.error.as_deref(),
            skill_loaded: played.skill_loaded,
            tools: &tools,
            input_tokens: i64::try_from(played.usage.input_tokens).unwrap_or(i64::MAX),
            output_tokens: i64::try_from(played.usage.output_tokens).unwrap_or(i64::MAX),
            cost_micros: played.usage.cost_micros,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(written)
}

/// Ends the evaluation, failed for `failure` when given, and audits what
/// its cases add up to; nothing when it was not running any more.
async fn finish(
    services: &Services,
    tenant_id: Uuid,
    id: Uuid,
    failure: Option<&str>,
) -> anyhow::Result<()> {
    let mut tx = services.db.begin(tenant_id).await?;
    if !finish_evaluation(&mut tx, id, failure).await? {
        return Ok(());
    }
    let Some(evaluation) = get_evaluation(&mut tx, id).await? else {
        return Ok(());
    };
    let results = results_of(&mut tx, &[id]).await?;
    let cases = evaluation.cases.as_array().map_or(0, Vec::len);
    let summary = summarize(&case_results(cases, &results));
    let mut entry = AuditEntry::new(
        tenant_id,
        "system",
        "skill.evaluated",
        if failure.is_some() {
            "failure"
        } else {
            "success"
        },
    )
    .resource("skill", evaluation.skill_id)
    .metadata(json!({
        "evaluationId": id,
        "version": evaluation.version,
        "referenceVersion": evaluation.reference_version,
        "cases": cases,
        "passed": summary.passed,
        "referencePassed": summary.reference_passed,
        "regressions": summary.regressions,
        "costMicros": evaluation.cost_micros,
    }));
    if let Some(launcher) = evaluation.launched_by_user_id {
        entry = entry.on_behalf_of(launcher);
    }
    entry.reason = failure.map(str::to_owned);
    append_audit(&mut tx, entry).await?;
    Ok(tx.commit().await?)
}
