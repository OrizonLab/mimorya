//! What Telegram sends to a company's bot (ADR-133), before any session:
//! the request names its gateway, then proves itself by the webhook's
//! secret, sent back in a header and compared with the one the platform
//! gave Telegram. An unknown gateway, a wrong or missing secret, and a
//! gateway whose secret this server cannot open answer alike. Once proven:
//! an update already taken (Telegram sends one again when it was not
//! answered in time), of a paused gateway, or anything but a person's own
//! text message in their private chat with the bot is taken and ignored.
//! A `/start` carrying a link code is that code sent back: the person who
//! wrote it is kept with the code, for the session that asked for it to
//! confirm (`link`). Anyone else is answered in their private chat: how to
//! link their account, or, once linked, that the conversation here is to
//! come. What a message says is never logged nor audited.

use chrono::{DateTime, Utc};
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::telegram::{
    PrivateMessage, Update, link_code_hash, read_update, secret_matches, start_code,
};
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_identities::find_gateway_identity;
use mimorya_db::repositories::gateway_link_codes::{Claim, claim_link_code};
use mimorya_db::repositories::gateways::{gateway_tenant, get_gateway, take_gateway_event};
use serde_json::json;
use uuid::Uuid;

use super::texts;
use crate::services::gateways::{GatewayService, bot_token_name, webhook_secret_name};

/// What became of an update Telegram sent.
#[derive(Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// Not the bot's webhook's: unknown gateway, secret missing or wrong,
    /// or a secret this server cannot open, alike.
    Unknown,
    /// Proven, but not what Telegram sends.
    Unreadable,
    /// Taken: acted upon, or ignored.
    Taken,
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
        let text = self
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

    /// What to answer `message`: a link code sent back is kept with whom it
    /// came from, and audited.
    async fn answer(
        &self,
        tx: &mut TenantTx,
        (tenant_id, gateway_id): (Uuid, Uuid),
        message: &PrivateMessage,
        now: DateTime<Utc>,
    ) -> anyhow::Result<String> {
        let person = message.person.id.to_string();
        match start_code(&message.text) {
            Some(Some(code)) => {
                let claim = Claim {
                    external_id: person.clone(),
                    name: message.person.name.clone(),
                };
                let hash = link_code_hash(code);
                let Some(owner) = claim_link_code(tx, gateway_id, &hash, now, &claim).await? else {
                    return Ok(texts::CODE_INVALID.to_owned());
                };
                let entry =
                    AuditEntry::new(tenant_id, "system", "gateway.link_code_claimed", "success")
                        .on_behalf_of(owner)
                        .resource("gateway", gateway_id)
                        .metadata(json!({ "externalId": person }));
                append_audit(tx, entry).await?;
                Ok(texts::CLAIMED.to_owned())
            }
            _ => Ok(
                match find_gateway_identity(tx, gateway_id, &person).await? {
                    Some(_) => texts::linked(&self.assistant_name),
                    None => texts::unlinked(&self.assistant_name),
                },
            ),
        }
    }
}
