//! Opening a conversation turn: the user message, the run that will answer it
//! and the history replayed to the model, recorded in one transaction.

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::DEFAULT_BUDGET;
use mimorya_core::DomainError;
use mimorya_core::authz::{HOOK_CHANNEL, WEB_CHANNEL};
use mimorya_core::autonomy::AutonomyLevel;
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::conversations::{
    ConversationPatch, DEFAULT_CONVERSATION_TITLE, MessageRow, NewMessage, add_message,
    list_messages, lock_conversation, update_conversation,
};
use mimorya_db::repositories::runs::{
    NewRun, RunRow, RunStatus, create_run, list_runs_for_conversation,
};
use mimorya_db::repositories::scheduled_tasks::link_started_run;
use serde_json::json;
use uuid::Uuid;

use crate::contract::MessageMetadata;
use crate::services::access::{AccessContext, AuthIdentity};

const HISTORY_LIMIT: i64 = 20;
const TITLE_LENGTH: usize = 80;

/// Who sends a turn: its user, or the platform for them (ADR-080).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnSender {
    User,
    /// Their scheduled task `task_id`, fired when due.
    Schedule {
        task_id: Uuid,
    },
    /// Their inbound hook `hook_id`, reached by another system (ADR-109).
    Hook {
        hook_id: Uuid,
    },
}

pub struct OpenedTurn {
    pub run: RunRow,
    pub history: Vec<LlmMessage>,
    pub user_message_id: Uuid,
}

pub async fn open_conversation_turn(
    tx: &mut TenantTx,
    identity: AuthIdentity,
    conversation_id: Uuid,
    text: &str,
    autonomy: AutonomyLevel,
    sender: TurnSender,
) -> anyhow::Result<OpenedTurn> {
    // Locked: a concurrent turn waits here, then sees the run opened by this one.
    let conversation = lock_conversation(tx, conversation_id)
        .await?
        .filter(|c| c.owner_user_id == identity.user_id)
        .ok_or_else(|| DomainError::not_found("Conversation introuvable"))?;
    let latest = list_runs_for_conversation(tx, conversation_id, 1).await?;
    if latest.first().is_some_and(|run| {
        matches!(
            run.status(),
            RunStatus::Queued | RunStatus::Running | RunStatus::AwaitingApproval
        )
    }) {
        return Err(DomainError::conflict(
            "Une demande est déjà en cours dans cette conversation : attendez sa fin ou sa validation.",
        )
        .into());
    }

    let previous = list_messages(tx, conversation_id, HISTORY_LIMIT).await?;
    let user_message = add_message(
        tx,
        NewMessage {
            tenant_id: identity.tenant_id,
            conversation_id,
            role: "user",
            content: text.to_string(),
            author_user_id: Some(identity.user_id),
            run_id: None,
            metadata: serde_json::to_value(MessageMetadata {
                inbound_hook_id: match sender {
                    TurnSender::Hook { hook_id } => Some(hook_id),
                    TurnSender::User | TurnSender::Schedule { .. } => None,
                },
                ..MessageMetadata::default()
            })?,
        },
    )
    .await?;
    if conversation.title == DEFAULT_CONVERSATION_TITLE {
        let title: String = text.chars().take(TITLE_LENGTH).collect();
        update_conversation(
            tx,
            conversation_id,
            ConversationPatch {
                title: Some(title),
                ..ConversationPatch::default()
            },
        )
        .await?;
    }

    let run = create_run(
        tx,
        NewRun {
            tenant_id: identity.tenant_id,
            conversation_id: Some(conversation_id),
            initiated_by_user_id: identity.user_id,
            on_behalf_of_user_id: identity.user_id,
            objective: text.to_string(),
            success_criteria: Vec::new(),
            autonomy_level: autonomy,
            budget: serde_json::to_value(DEFAULT_BUDGET)?,
            request_message_id: Some(user_message.id),
            mission_id: None,
            // Read again whenever the run resumes (ADR-109).
            channel: match sender {
                TurnSender::Hook { .. } => HOOK_CHANNEL,
                TurnSender::User | TurnSender::Schedule { .. } => WEB_CHANNEL,
            },
        },
    )
    .await?;
    // Its task's last run before it can run (ADR-080, ADR-082).
    if let TurnSender::Schedule { task_id } = sender {
        link_started_run(tx, task_id, conversation_id, run.id).await?;
    }
    let action = "agent.run.started";
    let started = match sender {
        TurnSender::User => {
            AuditEntry::new(identity.tenant_id, "user", action, "success").by_user(identity.user_id)
        }
        // Started by the platform when due, as its author asked beforehand.
        TurnSender::Schedule { task_id } => {
            AuditEntry::new(identity.tenant_id, "system", action, "success")
                .on_behalf_of(identity.user_id)
                .metadata(json!({ "scheduledTaskId": task_id }))
        }
        // Started when another system posted to their hook.
        TurnSender::Hook { hook_id } => {
            AuditEntry::new(identity.tenant_id, "system", action, "success")
                .on_behalf_of(identity.user_id)
                .metadata(json!({ "inboundHookId": hook_id }))
        }
    };
    append_audit(
        tx,
        started
            .run(run.id)
            .resource("conversation", conversation_id),
    )
    .await?;
    Ok(OpenedTurn {
        run,
        history: to_history(previous),
        user_message_id: user_message.id,
    })
}

/// Autonomy level configured for the tenant, `execute_with_approval` by default.
pub fn tenant_autonomy_level(access: &AccessContext) -> AutonomyLevel {
    access
        .tenant
        .settings
        .get("defaultAutonomyLevel")
        .and_then(|v| v.as_str())
        .and_then(AutonomyLevel::parse)
        .unwrap_or(AutonomyLevel::ExecuteWithApproval)
}

/// What a queued turn's run is first given (ADR-085): its conversation's
/// earlier messages, as the turn that queued it read them, and its request,
/// named on the run. `None` when the request is no longer there.
pub async fn queued_turn_history(
    tx: &mut TenantTx,
    run: &RunRow,
) -> anyhow::Result<Option<(Vec<LlmMessage>, String)>> {
    let (Some(conversation_id), Some(request_id)) = (run.conversation_id, run.request_message_id)
    else {
        return Ok(None);
    };
    // The turn read the conversation's last messages, then added its
    // request: nothing else is added until its run runs.
    let mut messages = list_messages(tx, conversation_id, HISTORY_LIMIT + 1).await?;
    let Some(at) = messages.iter().position(|m| m.id == request_id) else {
        return Ok(None);
    };
    let request = messages.remove(at);
    Ok(Some((to_history(messages), request.content)))
}

/// Stored messages replayed as plain text turns (tool exchanges stay in the run).
fn to_history(rows: Vec<MessageRow>) -> Vec<LlmMessage> {
    rows.into_iter()
        .filter(|m| !m.content.trim().is_empty())
        .map(|m| {
            if m.role == "user" {
                LlmMessage::user(m.content)
            } else {
                LlmMessage::assistant(m.content)
            }
        })
        .collect()
}
