//! Firing the scheduled tasks due (ADR-080), in the process that executes
//! runs: each occurrence is claimed first (its task rescheduled, the row
//! locked), so that it never fires twice; then its instruction is sent as
//! its author's, in a new conversation of theirs, with their rights as they
//! stand now. An occurrence does not run while the last one's run is still
//! under way, when its author's account cannot act, or more than an hour
//! late (the service was stopped: the request may have lost its object).
//! What became of it is recorded for its author, and audited.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_core::error::ErrorCode;
use mimorya_core::schedule::WeeklySchedule;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::conversations::{
    ConversationPatch, create_conversation, update_conversation,
};
use mimorya_db::repositories::scheduled_tasks::{
    DueTask, Occurrence, ScheduledTaskRow, due_scheduled_tasks, lock_due_task, record_occurrence,
    reschedule_task,
};
use serde_json::json;
use uuid::Uuid;

use crate::app::Services;
use crate::services::access::AuthIdentity;

/// Tasks fired per pass at most: the next pass, a minute later, goes on.
const BATCH: i32 = 50;
/// How late an occurrence may still run, once the service is back.
pub const CATCH_UP: chrono::Duration = chrono::Duration::hours(1);

const MISSED: &str = "Le service était arrêté à l’heure prévue : cette occurrence n’a pas été \
     rattrapée.";
const CANNOT_ACT: &str = "Votre compte ne pouvait pas agir à l’heure prévue (désactivé, ou \
     mot de passe provisoire à remplacer).";
const STILL_RUNNING: &str = "La précédente occurrence n’était pas terminée (en cours ou en \
     attente de validation) : celle-ci n’a pas été lancée.";
const NOT_STARTED: &str = "Le démarrage a échoué : la tâche reprendra à sa prochaine heure.";

/// Fires the tasks due by `now`; how many started a run.
pub async fn fire_due_tasks(services: &Services, now: DateTime<Utc>) -> anyhow::Result<usize> {
    let mut started = 0;
    for (tenant_id, task_id) in due_scheduled_tasks(services.db.pool(), now, BATCH).await? {
        match fire_task(services, tenant_id, task_id, now).await {
            Ok(true) => started += 1,
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(task = %task_id, error = format!("{error:#}"), "Scheduled task not fired");
            }
        }
    }
    Ok(started)
}

/// Fires the task if it is still due: listed due, it may have fired or
/// changed since (another pass, its author); whether a run started.
pub async fn fire_task(
    services: &Services,
    tenant_id: Uuid,
    task_id: Uuid,
    now: DateTime<Utc>,
) -> anyhow::Result<bool> {
    let mut tx = services.db.begin(tenant_id).await?;
    let Some(due) = lock_due_task(&mut tx, task_id, now).await? else {
        return Ok(false);
    };
    let task = due.task.clone();
    let next = schedule_of(&task).map(|schedule| schedule.next_after(now));
    reschedule_task(&mut tx, task.id, next).await?;
    if let Some((outcome, problem)) = held_back(&due, now) {
        record(&mut tx, tenant_id, &task, now, outcome, Some(problem), None).await?;
        tx.commit().await?;
        return Ok(false);
    }
    tx.commit().await?;

    let started = start(services, tenant_id, &task).await;
    let mut tx = services.db.begin(tenant_id).await?;
    let fired = match &started {
        Ok(ids) => {
            record(&mut tx, tenant_id, &task, now, "started", None, Some(*ids)).await?;
            true
        }
        Err(error) => {
            let (outcome, problem) = match error.downcast_ref::<DomainError>() {
                Some(refusal) if matches!(refusal.code, ErrorCode::Forbidden) => {
                    ("refused", refusal.message.as_str())
                }
                Some(refusal) => ("failed", refusal.message.as_str()),
                None => {
                    tracing::warn!(task = %task.id, error = format!("{error:#}"), "Scheduled run not started");
                    ("failed", NOT_STARTED)
                }
            };
            record(&mut tx, tenant_id, &task, now, outcome, Some(problem), None).await?;
            false
        }
    };
    tx.commit().await?;
    Ok(fired)
}

/// Its schedule as stored, checked when written; `None` pauses a task that
/// could no longer be read.
fn schedule_of(task: &ScheduledTaskRow) -> Option<WeeklySchedule> {
    let days: Vec<u8> = task
        .weekdays
        .iter()
        .filter_map(|d| u8::try_from(*d).ok())
        .collect();
    WeeklySchedule::parse(&days, &task.time_of_day, &task.time_zone).ok()
}

/// Why the occurrence does not run, if it does not.
fn held_back(due: &DueTask, now: DateTime<Utc>) -> Option<(&'static str, &'static str)> {
    let scheduled_for = due.task.next_run_at.unwrap_or(now);
    if now - scheduled_for > CATCH_UP {
        return Some(("missed", MISSED));
    }
    if due.owner_status != "active" || due.must_change_password {
        return Some(("refused", CANNOT_ACT));
    }
    if matches!(
        due.last_run_status.as_deref(),
        Some("queued" | "running" | "awaiting_approval")
    ) {
        return Some(("still_running", STILL_RUNNING));
    }
    None
}

/// Sends the instruction as its author's, in a new conversation of theirs,
/// once a model can answer and they may use the agent: nothing is opened
/// otherwise. A conversation whose run then fails to start is archived (the
/// application deletes none).
async fn start(
    services: &Services,
    tenant_id: Uuid,
    task: &ScheduledTaskRow,
) -> anyhow::Result<(Uuid, Uuid)> {
    // No session: the run reads the account's rights as they stand.
    let identity = AuthIdentity {
        tenant_id,
        user_id: task.user_id,
        session_id: Uuid::nil(),
        must_change_password: false,
    };
    services.runs.ready_to_run(identity).await?;
    let mut tx = services.db.begin(tenant_id).await?;
    let conversation =
        create_conversation(&mut tx, tenant_id, task.user_id, Some(&task.title)).await?;
    tx.commit().await?;
    let turn = services
        .runs
        .start_conversation_turn(identity, conversation.id, &task.instruction)
        .await;
    match turn {
        Ok(turn) => Ok((conversation.id, turn.run_id)),
        Err(error) => {
            let mut tx = services.db.begin(tenant_id).await?;
            let archived = ConversationPatch {
                title: None,
                archived_at: Some(Some(Utc::now())),
            };
            update_conversation(&mut tx, conversation.id, archived).await?;
            tx.commit().await?;
            Err(error)
        }
    }
}

/// Records an occurrence for its author and audits it, never what it asks.
async fn record(
    tx: &mut mimorya_db::TenantTx,
    tenant_id: Uuid,
    task: &ScheduledTaskRow,
    at: DateTime<Utc>,
    outcome: &str,
    problem: Option<&str>,
    started: Option<(Uuid, Uuid)>,
) -> anyhow::Result<()> {
    let occurrence = Occurrence {
        at,
        outcome,
        problem,
        conversation_id: started.map(|(conversation, _)| conversation),
        run_id: started.map(|(_, run)| run),
    };
    record_occurrence(tx, task.id, occurrence).await?;
    let (action, result) = match started {
        Some(_) => ("scheduled_task.fired", "success"),
        None => ("scheduled_task.not_run", "failure"),
    };
    let mut entry = AuditEntry::new(tenant_id, "system", action, result)
        .on_behalf_of(task.user_id)
        .resource("scheduled_task", task.id)
        .metadata(json!({ "outcome": outcome }));
    if let Some((_, run_id)) = started {
        entry = entry.run(run_id);
    }
    append_audit(tx, entry).await?;
    Ok(())
}
