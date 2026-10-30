//! What the company's bot answers a person's own message in their private
//! chat (ADR-133, ADR-135). A `/start` carrying a link code is that code
//! sent back: the bot names whose account it links and offers it to them,
//! never when their Telegram account is linked to someone else; their
//! earlier offers are withdrawn. `/oui` accepts the one offer awaiting
//! them, the last link the bot named, whose account it names again: they
//! are kept with it, for the session that asked for it to confirm (`link`).
//! So no one's Telegram account is linked to another's account unless they
//! accepted it knowing whose, nor to their own unless their session saw
//! whom it came from. Anyone else is told how to link their account. A
//! linked person's words go to the agent (`conversations`); `/nouvelle`
//! closes their conversation here, its words, if any, opening the next; any
//! other command (`/start`, `/aide`) tells them to whose account theirs is
//! linked and how to talk with the agent.

use chrono::{DateTime, Utc};
use mimorya_core::gateways::telegram::{PrivateMessage, is_consent, link_code_hash, start_code};
use mimorya_core::gateways::{command, new_conversation};
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_identities::find_gateway_identity;
use mimorya_db::repositories::gateway_link_codes::{
    Claim, accept_offered_code, link_code_owner, offer_link_code,
};
use mimorya_db::repositories::gateway_threads::close_gateway_chat;
use mimorya_db::repositories::users::get_user;
use serde_json::json;
use uuid::Uuid;

use super::texts;
use crate::services::gateways::GatewayService;

/// What becomes of a person's message to the bot.
pub(super) enum Answer {
    /// The bot says this, in plain text.
    Say(String),
    /// These words of a linked person go to the agent (ADR-135).
    Converse(String),
}

impl GatewayService {
    /// What becomes of `message`.
    pub(super) async fn answer(
        &self,
        tx: &mut TenantTx,
        (tenant_id, gateway_id): (Uuid, Uuid),
        message: &PrivateMessage,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Answer> {
        let person = message.person.id.to_string();
        let linked = (find_gateway_identity(tx, gateway_id, &person).await?)
            .map(|identity| identity.user_id);
        let ids = (tenant_id, gateway_id);
        if let Some(Some(code)) = start_code(&message.text) {
            return Ok(Answer::Say(
                offer(tx, ids, &person, linked, code, now).await?,
            ));
        }
        if is_consent(&message.text) {
            let claim = Claim {
                external_id: person,
                name: message.person.name.clone(),
            };
            return Ok(Answer::Say(accept(tx, ids, &claim, now).await?));
        }
        let Some(user_id) = linked else {
            return Ok(Answer::Say(texts::unlinked(&self.assistant_name)));
        };
        if let Some(first) = new_conversation(&message.text) {
            close_gateway_chat(tx, gateway_id, &person, user_id).await?;
            return Ok(if first.is_empty() {
                Answer::Say(texts::NEW_CONVERSATION_NEXT.to_owned())
            } else {
                Answer::Converse(first.to_owned())
            });
        }
        if command(&message.text).is_some() {
            let name = name_of(tx, user_id).await?;
            return Ok(Answer::Say(texts::help(&self.assistant_name, &name)));
        }
        Ok(Answer::Converse(message.text.clone()))
    }
}

/// Offers the code sent back to `person`, unless it links nothing or their
/// Telegram account is linked to someone else: whose account it links.
async fn offer(
    tx: &mut TenantTx,
    (tenant_id, gateway_id): (Uuid, Uuid),
    person: &str,
    linked: Option<Uuid>,
    code: &str,
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    let hash = link_code_hash(code);
    let Some(owner) = link_code_owner(tx, gateway_id, &hash, now).await? else {
        return Ok(texts::CODE_INVALID.to_owned());
    };
    if linked.is_some_and(|user_id| user_id != owner) {
        return Ok(texts::LINKED_ELSEWHERE.to_owned());
    }
    if offer_link_code(tx, gateway_id, &hash, now, person).await? != Some(owner) {
        return Ok(texts::CODE_INVALID.to_owned());
    }
    let entry = AuditEntry::new(tenant_id, "system", "gateway.link_code_offered", "success")
        .on_behalf_of(owner)
        .resource("gateway", gateway_id)
        .metadata(json!({ "externalId": person }));
    append_audit(tx, entry).await?;
    Ok(texts::offer(&name_of(tx, owner).await?))
}

/// Accepts the one offer awaiting whoever `claim` names: kept with it for
/// the session that asked for it to confirm.
async fn accept(
    tx: &mut TenantTx,
    (tenant_id, gateway_id): (Uuid, Uuid),
    claim: &Claim,
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    let Some(owner) = accept_offered_code(tx, gateway_id, now, claim).await? else {
        return Ok(texts::NOTHING_OFFERED.to_owned());
    };
    let entry = AuditEntry::new(tenant_id, "system", "gateway.link_code_claimed", "success")
        .on_behalf_of(owner)
        .resource("gateway", gateway_id)
        .metadata(json!({ "externalId": claim.external_id }));
    append_audit(tx, entry).await?;
    Ok(texts::claimed(&name_of(tx, owner).await?))
}

/// The name the company gave the person `user_id`.
async fn name_of(tx: &mut TenantTx, user_id: Uuid) -> anyhow::Result<String> {
    let user = get_user(tx, user_id).await?;
    Ok(user.map_or_else(String::new, |user| user.display_name))
}
