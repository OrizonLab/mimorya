//! What Telegram sends to a company's bot (ADR-133), before any session:
//! the request names its gateway, then proves itself by the webhook's
//! secret, sent back in a header and compared with the one the platform
//! gave Telegram. An unknown gateway, a wrong or missing secret, and a
//! gateway whose secret this server cannot open answer alike. Once proven:
//! an update already taken (Telegram sends one again when it was not
//! answered in time), of a paused gateway, or anything but a person's own
//! text message in their private chat with the bot is taken and ignored.
//! The message is then answered in the person's private chat (`answers`);
//! a linked person's words, once the update is answered, by the agent
//! (`conversations`, ADR-135). What a message says is never logged nor
//! audited.

use chrono::{DateTime, Utc};
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::telegram::{Update, read_update, secret_matches};
use mimorya_db::repositories::gateways::{gateway_tenant, get_gateway, take_gateway_event};
use uuid::Uuid;

use super::answers::Answer;
use crate::services::gateways::{GatewayService, bot_token_name, webhook_secret_name};

/// A linked person's words to the bot (ADR-135): to answer in a
/// conversation of theirs, once the update is answered.
pub struct ChatMessage {
    pub tenant_id: Uuid,
    pub gateway_id: Uuid,
    /// Who wrote: their Telegram identifier, also their private chat's.
    pub person: i64,
    /// Their message's number in the chat.
    pub message_id: i64,
    /// Their words for the agent.
    pub text: String,
    /// The bot's token, to answer with.
    pub token: String,
}

/// Never the token nor the words, whatever prints it.
impl std::fmt::Debug for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatMessage")
            .field("gateway_id", &self.gateway_id)
            .finish_non_exhaustive()
    }
}

/// What became of an update Telegram sent.
#[derive(Debug)]
pub enum UpdateOutcome {
    /// Not the bot's webhook's: unknown gateway, secret missing or wrong,
    /// or a secret this server cannot open, alike.
    Unknown,
    /// Proven, but not what Telegram sends.
    Unreadable,
    /// Taken: acted upon, or ignored.
    Taken,
    /// Taken: a linked person's words, for the agent to answer (ADR-135).
    Message(Box<ChatMessage>),
}

impl GatewayService {
    /// Takes what Telegram sent to the gateway `gateway_id` with the secret
    /// `sent`, received at `now`.
    pub async fn receive_telegram(
        &self,
        gateway_id: Uuid,
        sent: Option<&str>,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> anyhow::Result<UpdateOutcome> {
        let kind = GatewayKind::Telegram.as_str();
        let Some(tenant_id) = gateway_tenant(self.db.pool(), gateway_id, kind).await? else {
            return Ok(UpdateOutcome::Unknown);
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(gateway) = get_gateway(&mut tx, gateway_id).await? else {
            return Ok(UpdateOutcome::Unknown);
        };
        let name = webhook_secret_name(gateway_id);
        let Some(secret) = self.secret(&mut tx, tenant_id, &name).await? else {
            // No master key, or another one: told the operator, answered to
            // the sender as an unknown gateway.
            tracing::warn!(gateway = %gateway_id, "Telegram gateway's webhook secret unreadable");
            return Ok(UpdateOutcome::Unknown);
        };
        if !secret_matches(&secret, sent) {
            return Ok(UpdateOutcome::Unknown);
        }
        let Some(update) = read_update(body) else {
            return Ok(UpdateOutcome::Unreadable);
        };
        let event_id = update.update_id.to_string();
        if !gateway.enabled
            || !take_gateway_event(&mut tx, tenant_id, gateway_id, &event_id).await?
        {
            return Ok(UpdateOutcome::Taken);
        }
        let Update::Message(message) = update.content else {
            tx.commit().await?;
            return Ok(UpdateOutcome::Taken);
        };
        let answer = self
            .answer(&mut tx, (tenant_id, gateway_id), &message, now)
            .await?;
        let token = self
            .secret(&mut tx, tenant_id, &bot_token_name(gateway_id))
            .await?;
        tx.commit().await?;
        let Some(token) = token else {
            tracing::warn!(gateway = %gateway_id, "Telegram gateway's bot token unreadable");
            return Ok(UpdateOutcome::Taken);
        };
        let text = match answer {
            Answer::Say(text) => text,
            Answer::Converse(text) => {
                return Ok(UpdateOutcome::Message(Box::new(ChatMessage {
                    tenant_id,
                    gateway_id,
                    person: message.person.id,
                    message_id: message.message_id,
                    text,
                    token,
                })));
            }
        };
        let (telegram, chat) = (self.telegram.clone(), message.person.id);
        self.background.spawn(async move {
            // In their private chat with the bot, which their identifier
            // names: never anyone else's.
            if let Err(failure) = telegram.send(&token, chat, &text).await {
                tracing::warn!(gateway = %gateway_id, failure = %failure.message(), "Telegram answer not sent");
            }
        });
        Ok(UpdateOutcome::Taken)
    }
}
