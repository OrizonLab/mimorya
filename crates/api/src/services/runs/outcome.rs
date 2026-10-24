//! Persisting what a run produced: its status and usage, the assistant message
//! shown in the conversation, and the audit entry; in the transaction its
//! execution holds its lease in (ADR-086), which carries its mission on as
//! the stage ends (ADR-089). A crash is recorded only for a run still
//! running under this executor's lease, or already finished.

use chrono::Utc;
use mimorya_agent::runtime::{RunCheckpoint, RunOutcome};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::conversations::{NewMessage, add_message};
use mimorya_db::repositories::run_events::run_end_recorded;
use mimorya_db::repositories::run_leases::lock_held_lease;
use mimorya_db::repositories::runs::{RunPatch, RunStatus, get_run, update_active_run};
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::db_ports::RunScope;
use crate::contract::{self, MessageMetadata, MissionPart};
use crate::services::event_bus::RunEventBus;
use crate::services::missions::after_stage;

const MAX_EVIDENCE_IN_MESSAGE: usize = 50;

/// Why a run that crashed failed, as recorded.
const INTERNAL_ERROR: &str = "Erreur interne";

/// Shown when a run crashed: the internal error is logged, never displayed.
pub const CRASH_MESSAGE: &str = "Une erreur interne a interrompu le traitement. Réessayez ; si le problème persiste, contactez votre administrateur.";

/// Saves `outcome` in `tx`, which commits it, with what follows for the
/// run's mission: its next stage, queued, is announced on `bus` once
/// committed.
pub async fn persist_outcome(
    mut tx: TenantTx,
    bus: &RunEventBus,
    scope: RunScope,
    outcome: &RunOutcome,
) -> anyhow::Result<()> {
    record_outcome(&mut tx, scope, outcome).await?;
    let next = after_stage(&mut tx, bus, scope, outcome).await?;
    tx.commit().await?;
    if let Some((run_id, queued)) = next {
        bus.publish(run_id, queued);
    }
    Ok(())
}

async fn record_outcome(
    tx: &mut TenantTx,
    scope: RunScope,
    outcome: &RunOutcome,
) -> anyhow::Result<()> {
    let checkpoint = outcome.checkpoint();
    let mut patch = RunPatch {
        state: Some(serde_json::to_value(checkpoint)?),
        usage: Some(serde_json::to_value(checkpoint.usage)?),
        ..RunPatch::default()
    };

    let (content, verification, mission) = match outcome {
        RunOutcome::AwaitingApproval { .. } => {
            patch.status = Some(RunStatus::AwaitingApproval);
            if !update_active_run(tx, scope.run_id, patch).await? {
                anyhow::bail!("run {} already finished", scope.run_id);
            }
            return Ok(());
        }
        RunOutcome::Completed {
            answer,
            verification,
            ..
        } => {
            patch.status = Some(RunStatus::Completed);
            patch.result = Some(json!({ "answer": answer }));
            patch.verification = Some(serde_json::to_value(verification)?);
            (answer.clone(), Some(verification.into()), None)
        }
        RunOutcome::Handoff { note, .. } => {
            patch.status = Some(RunStatus::Completed);
            patch.result = Some(json!({ "progress": note }));
            (note.clone(), None, Some(MissionPart::Progress))
        }
        RunOutcome::Failed { error, .. } => {
            patch.status = Some(RunStatus::Failed);
            patch.error = Some(error.clone());
            (
                format!("Je n’ai pas pu terminer cette demande : {error}"),
                None,
                None,
            )
        }
    };
    patch.finished_at = Some(Utc::now());
    let status = patch.status.map(contract::RunStatus::from);
    // A run failed meanwhile (stale-run sweeper) keeps its status.
    if !update_active_run(tx, scope.run_id, patch).await? {
        anyhow::bail!("run {} already finished", scope.run_id);
    }

    if let Some(conversation_id) = scope.conversation_id {
        let metadata = MessageMetadata {
            status,
            evidence: Some(
                checkpoint
                    .evidence
                    .iter()
                    .take(MAX_EVIDENCE_IN_MESSAGE)
                    .map(Into::into)
                    .collect(),
            ),
            verification,
            mission,
        };
        add_message(
            tx,
            NewMessage {
                tenant_id: scope.tenant_id,
                conversation_id,
                role: "assistant",
                content,
                author_user_id: None,
                run_id: Some(scope.run_id),
                metadata: serde_json::to_value(metadata)?,
            },
        )
        .await?;
    }
    let succeeded = matches!(
        outcome,
        RunOutcome::Completed { .. } | RunOutcome::Handoff { .. }
    );
    append_audit(
        tx,
        AuditEntry::new(
            scope.tenant_id,
            "agent",
            format!("agent.run.{}", outcome.status()),
            if succeeded { "success" } else { "failure" },
        )
        .on_behalf_of(scope.user_id)
        .run(scope.run_id)
        .metadata(json!({ "usage": checkpoint.usage })),
    )
    .await?;
    Ok(())
}

/// Marks a run that crashed as failed, without exposing the internal error:
/// the conversation shows `message`. A run that already finished (outcome
/// saved before the crash, or failed by the stale-run sweep) keeps its
/// status. Returns the status to announce, or `None` when the run's end is
/// already announced (by the sweep, typically), or the run no longer runs
/// under `owner`'s lease and has not finished: another executor's, or
/// paused or queued since, it is left as it stands (ADR-086). A mission's
/// stage that crashed ends its mission (ADR-089).
pub async fn record_crash(
    db: &Database,
    bus: &RunEventBus,
    scope: RunScope,
    message: &str,
    owner: Uuid,
) -> anyhow::Result<Option<RunStatus>> {
    let mut tx = db.begin(scope.tenant_id).await?;
    if !lock_held_lease(&mut tx, scope.run_id, owner).await? {
        let run = get_run(&mut tx, scope.run_id).await?;
        if run.is_some_and(|r| !r.status().is_finished()) {
            return Ok(None);
        }
    }
    let failed = update_active_run(
        &mut tx,
        scope.run_id,
        RunPatch {
            status: Some(RunStatus::Failed),
            error: Some(INTERNAL_ERROR.into()),
            finished_at: Some(Utc::now()),
            ..RunPatch::default()
        },
    )
    .await?;
    if !failed {
        if run_end_recorded(&mut tx, scope.run_id).await? {
            return Ok(None);
        }
        let run = get_run(&mut tx, scope.run_id).await?;
        return Ok(Some(run.map_or(RunStatus::Failed, |r| r.status())));
    }
    if let Some(conversation_id) = scope.conversation_id {
        add_message(
            &mut tx,
            NewMessage {
                tenant_id: scope.tenant_id,
                conversation_id,
                role: "assistant",
                content: message.into(),
                author_user_id: None,
                run_id: Some(scope.run_id),
                metadata: serde_json::to_value(MessageMetadata {
                    status: Some(contract::RunStatus::Failed),
                    ..MessageMetadata::default()
                })?,
            },
        )
        .await?;
    }
    let crashed = RunOutcome::Failed {
        error: INTERNAL_ERROR.into(),
        checkpoint: RunCheckpoint::new(String::new(), Vec::new()),
    };
    after_stage(&mut tx, bus, scope, &crashed).await?;
    tx.commit().await?;
    Ok(Some(RunStatus::Failed))
}
