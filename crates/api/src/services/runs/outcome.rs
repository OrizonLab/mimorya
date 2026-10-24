//! Persisting what a run produced: its status and usage, the assistant message
//! shown in the conversation, and the audit entry; in the transaction its
//! execution holds its lease in (ADR-086). A crash is recorded only for a
//! run that does not run under another executor's lease.

use chrono::Utc;
use mimorya_agent::runtime::RunOutcome;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::conversations::{NewMessage, add_message};
use mimorya_db::repositories::run_events::run_end_recorded;
use mimorya_db::repositories::run_leases::lock_held_lease;
use mimorya_db::repositories::runs::{RunPatch, RunStatus, get_run, update_active_run};
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::db_ports::RunScope;
use crate::contract::{self, MessageMetadata};

const MAX_EVIDENCE_IN_MESSAGE: usize = 50;

/// Shown when a run crashed: the internal error is logged, never displayed.
pub const CRASH_MESSAGE: &str = "Une erreur interne a interrompu le traitement. Réessayez ; si le problème persiste, contactez votre administrateur.";

/// Saves `outcome` in `tx`, which commits it.
pub async fn persist_outcome(
    mut tx: TenantTx,
    scope: RunScope,
    outcome: &RunOutcome,
) -> anyhow::Result<()> {
    let checkpoint = outcome.checkpoint();
    let mut patch = RunPatch {
        state: Some(serde_json::to_value(checkpoint)?),
        usage: Some(serde_json::to_value(checkpoint.usage)?),
        ..RunPatch::default()
    };

    let (content, verification) = match outcome {
        RunOutcome::AwaitingApproval { .. } => {
            patch.status = Some(RunStatus::AwaitingApproval);
            if !update_active_run(&mut tx, scope.run_id, patch).await? {
                anyhow::bail!("run {} already finished", scope.run_id);
            }
            return Ok(tx.commit().await?);
        }
        RunOutcome::Completed {
            answer,
            verification,
            ..
        } => {
            patch.status = Some(RunStatus::Completed);
            patch.result = Some(json!({ "answer": answer }));
            patch.verification = Some(serde_json::to_value(verification)?);
            (answer.clone(), Some(verification.into()))
        }
        RunOutcome::Failed { error, .. } => {
            patch.status = Some(RunStatus::Failed);
            patch.error = Some(error.clone());
            (
                format!("Je n’ai pas pu terminer cette demande : {error}"),
                None,
            )
        }
    };
    patch.finished_at = Some(Utc::now());
    let status = patch.status.map(contract::RunStatus::from);
    // A run failed meanwhile (stale-run sweeper) keeps its status.
    if !update_active_run(&mut tx, scope.run_id, patch).await? {
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
        };
        add_message(
            &mut tx,
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
    let succeeded = matches!(outcome, RunOutcome::Completed { .. });
    append_audit(
        &mut tx,
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
    Ok(tx.commit().await?)
}

/// Marks a run that crashed as failed, without exposing the internal error:
/// the conversation shows `message`. A run that already finished (outcome
/// saved before the crash, or failed by the stale-run sweep) keeps its
/// status. Returns the status to announce, or `None` when the run's end is
/// already announced (by the sweep, typically), or the run is another
/// executor's than `owner` (its lease taken up meanwhile, ADR-086).
pub async fn record_crash(
    db: &Database,
    scope: RunScope,
    message: &str,
    owner: Uuid,
) -> anyhow::Result<Option<RunStatus>> {
    let mut tx = db.begin(scope.tenant_id).await?;
    if !lock_held_lease(&mut tx, scope.run_id, owner).await? {
        let run = get_run(&mut tx, scope.run_id).await?;
        if run.is_some_and(|r| r.status() == RunStatus::Running) {
            return Ok(None);
        }
    }
    let failed = update_active_run(
        &mut tx,
        scope.run_id,
        RunPatch {
            status: Some(RunStatus::Failed),
            error: Some("Erreur interne".into()),
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
    tx.commit().await?;
    Ok(Some(RunStatus::Failed))
}
