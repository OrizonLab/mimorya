//! Starting a turn: its user's, in one of their conversations or in a new
//! one held where they wrote, the Slack thread they opened (ADR-127) or
//! their private chat with the Telegram bot (ADR-135), or one the platform
//! sends for them in a new conversation of theirs: their scheduled task
//! when due (ADR-080), their inbound hook when another system posts to it
//! (ADR-109). Such a conversation opens in the transaction of its turn: a
//! run that does not start leaves none behind.

use std::sync::Arc;

use mimorya_core::DomainError;
use mimorya_core::autonomy::AutonomyLevel;
use mimorya_core::gateways::GatewayKind;
use mimorya_db::repositories::conversations::create_conversation;
use mimorya_db::repositories::gateway_threads::{NewGatewayThread, insert_gateway_thread};
use uuid::Uuid;

use super::conversation_turn::{BUSY, TurnSender, open_conversation_turn, tenant_autonomy_level};
use super::executor::ExecutionMode;
use super::service::RunService;
use crate::services::access::{AccessContext, AuthIdentity};
use crate::services::llm::Unusable;

pub const COMPANY_NOT_ACTIVE: &str =
    "Votre entreprise est suspendue : l’agent n’agit plus pour elle.";
pub struct StartedTurn {
    pub run_id: Uuid,
    pub message_id: Uuid,
}

/// Where a new conversation is held on a gateway: a Slack thread (ADR-127),
/// a person's private chat with a Telegram bot (ADR-135), or their personal
/// conversation with a Teams bot (ADR-140).
#[derive(Debug, Clone, Copy)]
pub struct GatewayThread<'a> {
    pub gateway_id: Uuid,
    pub kind: GatewayKind,
    /// Who wrote there.
    pub external_id: &'a str,
    /// Their direct conversation with the application, or their private
    /// chat with the bot.
    pub channel_id: &'a str,
    /// The thread: the timestamp of its first message in Slack, the number
    /// of the message that opens it in Telegram, that activity's identifier
    /// in Teams.
    pub thread_ts: &'a str,
    /// The Teams service its message came from (Teams only, ADR-140).
    pub service_url: Option<&'a str>,
}

/// Where a turn is sent.
enum Destination<'a> {
    /// The conversation `0`, its sender's.
    Conversation(Uuid),
    /// A new conversation of theirs, so titled, held in `thread` if any.
    New {
        title: &'a str,
        thread: Option<GatewayThread<'a>>,
    },
}

impl RunService {
    pub async fn start_conversation_turn(
        &self,
        identity: AuthIdentity,
        conversation_id: Uuid,
        text: &str,
        sender: TurnSender,
    ) -> anyhow::Result<StartedTurn> {
        let into = Destination::Conversation(conversation_id);
        let (_, started) = self.start_turn(identity, into, text, sender).await?;
        Ok(started)
    }

    /// Sends `text` for the account `user_id`, in a new conversation of
    /// theirs titled `title`, with their rights as they stand (no session):
    /// the conversation and its run.
    pub async fn start_for(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        title: &str,
        text: &str,
        sender: TurnSender,
    ) -> anyhow::Result<(Uuid, Uuid)> {
        let identity = AuthIdentity {
            tenant_id,
            user_id,
            session_id: Uuid::nil(),
            must_change_password: false,
        };
        let into = Destination::New {
            title,
            thread: None,
        };
        let (conversation_id, started) = self.start_turn(identity, into, text, sender).await?;
        Ok((conversation_id, started.run_id))
    }

    /// Sends `text`, written by `identity`'s person to the gateway of
    /// `thread` (ADR-127, ADR-135), in a new conversation of theirs titled
    /// `title`, held in `thread`: the conversation and its run. A thread (or
    /// a Telegram chat) a message of its own opened a conversation for
    /// meanwhile is refused as busy.
    pub async fn start_in_thread(
        &self,
        identity: AuthIdentity,
        thread: GatewayThread<'_>,
        title: &str,
        text: &str,
    ) -> anyhow::Result<(Uuid, Uuid)> {
        let into = Destination::New {
            title,
            thread: Some(thread),
        };
        let sender = TurnSender::Gateway {
            gateway_id: thread.gateway_id,
            kind: thread.kind,
        };
        let (conversation_id, started) = self.start_turn(identity, into, text, sender).await?;
        Ok((conversation_id, started.run_id))
    }

    async fn start_turn(
        &self,
        identity: AuthIdentity,
        into: Destination<'_>,
        text: &str,
        sender: TurnSender,
    ) -> anyhow::Result<(Uuid, StartedTurn)> {
        let access = self.ready_to_run(identity).await?;
        let autonomy = match sender {
            // Led by what another system posted: what it writes is validated (ADR-109).
            TurnSender::Hook { .. } => match tenant_autonomy_level(&access) {
                AutonomyLevel::AutonomousLimited => AutonomyLevel::ExecuteWithApproval,
                level => level,
            },
            TurnSender::User | TurnSender::Schedule { .. } | TurnSender::Gateway { .. } => {
                tenant_autonomy_level(&access)
            }
        };

        let mut tx = self.db.begin(identity.tenant_id).await?;
        let conversation_id = match into {
            Destination::Conversation(id) => id,
            Destination::New { title, thread } => {
                let (tenant_id, user_id) = (identity.tenant_id, identity.user_id);
                let id = create_conversation(&mut tx, tenant_id, user_id, Some(title))
                    .await?
                    .id;
                if let Some(thread) = thread {
                    let held = NewGatewayThread {
                        tenant_id,
                        gateway_id: thread.gateway_id,
                        user_id,
                        conversation_id: id,
                        external_id: thread.external_id,
                        channel_id: thread.channel_id,
                        thread_ts: thread.thread_ts,
                        service_url: thread.service_url,
                    };
                    // A message of the same thread, or chat, opened its
                    // conversation first.
                    if !insert_gateway_thread(&mut tx, held).await? {
                        return Err(DomainError::conflict(BUSY).into());
                    }
                }
                id
            }
        };
        let turn =
            open_conversation_turn(&mut tx, identity, conversation_id, text, autonomy, sender)
                .await?;
        let dispatch = self.dispatch(&mut tx, turn.run.id).await?;
        tx.commit().await?;

        let started = StartedTurn {
            run_id: turn.run.id,
            message_id: turn.user_message_id,
        };
        let mode = ExecutionMode::Start {
            history: turn.history,
            text: text.to_string(),
        };
        self.dispatched(turn.run.id, dispatch, || {
            self.launch(Arc::new(access), turn.run, mode);
        });
        Ok((conversation_id, started))
    }

    /// The rights of whom a run would serve, once a model can answer them,
    /// their company is active and they may use the agent: refused
    /// otherwise, saying why. A session of a suspended company is refused
    /// beforehand; a scheduled task (ADR-080) or a hook (ADR-109) has none.
    pub async fn ready_to_run(&self, identity: AuthIdentity) -> anyhow::Result<AccessContext> {
        match self.executor.llm.provider_for(identity.tenant_id).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                return Err(DomainError::unavailable(
                    "Aucun modèle n’est disponible : la plateforme n’en fournit pas et votre \
                     entreprise n’en a pas configuré (Administration › Modèle d’IA).",
                )
                .into());
            }
            Err(error) if !error.is::<Unusable>() => return Err(error),
            Err(_) => {
                return Err(DomainError::unavailable(
                    "Le modèle choisi par votre entreprise ne peut pas être utilisé : un \
                     administrateur doit revoir sa configuration (Administration › Modèle d’IA).",
                )
                .into());
            }
        }
        let access = self.access.context(identity).await?;
        if access.tenant.status != "active" {
            return Err(DomainError::forbidden(COMPANY_NOT_ACTIVE).into());
        }
        access.require("agent:use")?;
        Ok(access)
    }
}
