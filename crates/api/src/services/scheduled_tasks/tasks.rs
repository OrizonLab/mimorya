//! A person's own scheduled tasks (ADR-080): listed, created, changed,
//! paused and deleted by their author only; anyone else's answers as absent.
//! Creating or resuming one needs the right to use the agent; at most
//! `MAX_TASKS` each. Each change is audited, never the instruction's text.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::scheduled_tasks::{
    ScheduledTaskRow, TaskDefinition, count_scheduled_tasks, create_scheduled_task,
    delete_scheduled_task, list_scheduled_tasks, lock_scheduled_task, update_scheduled_task,
};
use serde_json::json;
use uuid::Uuid;

use super::definition::CheckedTask;
use crate::services::AccessContext;

/// The tasks one person may keep.
pub const MAX_TASKS: i64 = 20;
const PERMISSION: &str = "agent:use";

const UNKNOWN_TASK: &str = "Tâche planifiée introuvable";
const TOO_MANY: &str = "Vous avez déjà 20 tâches planifiées : supprimez-en une d’abord.";

/// A task as written: what it asks, when, and whether it runs.
pub struct TaskInput {
    pub title: String,
    pub instruction: String,
    pub weekdays: Vec<u8>,
    pub time: String,
    pub time_zone: String,
    pub enabled: bool,
}

/// What changes in a task; what is absent stays.
#[derive(Default)]
pub struct TaskChanges {
    pub title: Option<String>,
    pub instruction: Option<String>,
    pub weekdays: Option<Vec<u8>>,
    pub time: Option<String>,
    pub time_zone: Option<String>,
    pub enabled: Option<bool>,
}

pub async fn scheduled_tasks(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<Vec<ScheduledTaskRow>> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let tasks = list_scheduled_tasks(&mut tx, access.subject.user_id).await?;
    tx.commit().await?;
    Ok(tasks)
}

pub async fn create_task(
    db: &Database,
    access: &AccessContext,
    input: TaskInput,
    now: DateTime<Utc>,
) -> anyhow::Result<Uuid> {
    access.require(PERMISSION)?;
    let task = CheckedTask::check(
        &input.title,
        &input.instruction,
        &input.weekdays,
        &input.time,
        &input.time_zone,
    )?;
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    if count_scheduled_tasks(&mut tx, user_id).await? >= MAX_TASKS {
        return Err(DomainError::conflict(TOO_MANY).into());
    }
    let weekdays = task.weekdays();
    let time = task.schedule.time();
    let next_run_at = input.enabled.then(|| task.schedule.next_after(now));
    let definition = TaskDefinition {
        title: &task.title,
        instruction: &task.instruction,
        weekdays: &weekdays,
        time_of_day: &time,
        time_zone: task.schedule.zone(),
        next_run_at,
    };
    let id = create_scheduled_task(&mut tx, tenant_id, user_id, definition).await?;
    append_audit(
        &mut tx,
        audited(access, "scheduled_task.created", id, &task, input.enabled),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn change_task(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
    changes: TaskChanges,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let Some(current) = lock_scheduled_task(&mut tx, id, access.subject.user_id).await? else {
        return Err(DomainError::not_found(UNKNOWN_TASK).into());
    };
    let enabled = changes.enabled.unwrap_or(current.next_run_at.is_some());
    // Pausing needs no right: one who lost the agent may still stop theirs.
    if enabled {
        access.require(PERMISSION)?;
    }
    let weekdays: Vec<u8> = current
        .weekdays
        .iter()
        .filter_map(|d| u8::try_from(*d).ok())
        .collect();
    let task = CheckedTask::check(
        changes.title.as_deref().unwrap_or(&current.title),
        changes
            .instruction
            .as_deref()
            .unwrap_or(&current.instruction),
        changes.weekdays.as_deref().unwrap_or(&weekdays),
        changes.time.as_deref().unwrap_or(&current.time_of_day),
        changes.time_zone.as_deref().unwrap_or(&current.time_zone),
    )?;
    let weekdays = task.weekdays();
    let time = task.schedule.time();
    // When it runs unchanged, its next occurrence stays, even one due that
    // has not fired yet.
    let unchanged = weekdays == current.weekdays
        && time == current.time_of_day
        && task.schedule.zone() == current.time_zone;
    let next_run_at = match current.next_run_at {
        Some(next) if enabled && unchanged => Some(next),
        _ => enabled.then(|| task.schedule.next_after(now)),
    };
    let definition = TaskDefinition {
        title: &task.title,
        instruction: &task.instruction,
        weekdays: &weekdays,
        time_of_day: &time,
        time_zone: task.schedule.zone(),
        next_run_at,
    };
    update_scheduled_task(&mut tx, id, definition).await?;
    append_audit(
        &mut tx,
        audited(access, "scheduled_task.updated", id, &task, enabled),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete_task(db: &Database, access: &AccessContext, id: Uuid) -> anyhow::Result<()> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    if !delete_scheduled_task(&mut tx, id, access.subject.user_id).await? {
        return Err(DomainError::not_found(UNKNOWN_TASK).into());
    }
    let entry = AuditEntry::new(
        access.subject.tenant_id,
        "user",
        "scheduled_task.deleted",
        "success",
    )
    .by_user(access.subject.user_id)
    .resource("scheduled_task", id);
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

/// A change of the task `id`, audited with when it runs: never what it asks.
fn audited(
    access: &AccessContext,
    action: &str,
    id: Uuid,
    task: &CheckedTask,
    enabled: bool,
) -> AuditEntry {
    AuditEntry::new(access.subject.tenant_id, "user", action, "success")
        .by_user(access.subject.user_id)
        .resource("scheduled_task", id)
        .metadata(json!({
            "weekdays": task.schedule.days(),
            "time": task.schedule.time(),
            "timeZone": task.schedule.zone(),
            "enabled": enabled,
        }))
}
