//! Opening a conversation turn: the user message, the run that will answer it
//! and the history replayed to the model (`history.rs`), recorded in one
//! transaction.

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::DEFAULT_BUDGET;
use mimorya_core::DomainError;
use mimorya_core::authz::{
    HOOK_CHANNEL, SLACK_CHANNEL, TEAMS_CHANNEL, TELEGRAM_CHANNEL, WEB_CHANNEL,
};
use mimorya_core::autonomy::AutonomyLevel;
use mimorya_core::gateways::GatewayKind;
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::conversations::{
    ConversationPatch, DEFAULT_CONVERSATION_TITLE, NewMessage, add_message, lock_conversation,
    update_conversation,
};
use mimorya_db::repositories::gateway_threads::touch_gateway_thread;
use mimorya_db::repositories::runs::{
    NewRun, RunRow, RunStatus, create_run, list_runs_for_conversation,
};
use mimorya_db::repositories::scheduled_tasks::link_started_run;
use serde_json::json;
use uuid::Uuid;

use super::history::conversation_history;
use crate::contract::MessageMetadata;
use crate::services::access::{AccessContext, AuthIdentity};

/// How much of a person's first words titles their conversation.
pub const TITLE_LENGTH: usize = 80;
/// A turn sent while the conversation's request is still in progress.
pub(super) const BUSY: &str =
    "Une demande est déjà en cours dans cette conversation : attendez sa fin ou sa validation.";

/// Who sends a turn: its user, from the application or from one of their
/// company's gateways (ADR-127, ADR-135), or the platform for them (ADR-080).
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
    /// Its user, writing to the gateway `gateway_id` of the service `kind`:
    /// their company's Slack application (ADR-127) or Telegram bot (ADR-135).
    Gateway {
        gateway_id: Uuid,
        kind: GatewayKind,
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
        return Err(DomainError::conflict(BUSY).into());
    }

    let history = conversation_history(tx, conversation_id).await?;
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
                    _ => None,
                },
                gateway: match sender {
                    TurnSender::Gateway { kind, .. } => Some(kind.as_str().to_owned()),
                    _ => None,
                },
                ..MessageMetadata::default()
            })?,
        },
    )
    .await?;
    // A person's words title their conversation while it bears the default
    // title. The platform's turns never do: in a conversation it opened with
    // a task's or a hook's title, which may be that default, a hook's turn
    // carries what was posted, which must not name it, nor leave the
    // platform with that name (ADR-111). The author's own reply there
    // still may.
    if conversation.title == DEFAULT_CONVERSATION_TITLE && matches!(sender, TurnSender::User) {
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
                TurnSender::Gateway { kind, .. } => match kind {
                    GatewayKind::Slack => SLACK_CHANNEL,
                    GatewayKind::Telegram => TELEGRAM_CHANNEL,
                    GatewayKind::Teams => TEAMS_CHANNEL,
                },
                TurnSender::User | TurnSender::Schedule { .. } => WEB_CHANNEL,
            },
        },
    )
    .await?;
    // Its task's last run before it can run (ADR-080, ADR-082).
    if let TurnSender::Schedule { task_id } = sender {
        link_started_run(tx, task_id, conversation_id, run.id).await?;
    }
    // Where it is held is active: its answers are written there (ADR-127,
    // ADR-135).
    if let TurnSender::Gateway { .. } = sender {
        touch_gateway_thread(tx, conversation_id).await?;
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
        // Written by them, in Slack or Telegram.
        TurnSender::Gateway { gateway_id, .. } => {
            AuditEntry::new(identity.tenant_id, "user", action, "success")
                .by_user(identity.user_id)
                .metadata(json!({ "gatewayId": gateway_id }))
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
        history,
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
