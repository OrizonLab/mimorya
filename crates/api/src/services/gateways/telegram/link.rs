//! A person links their Telegram account to theirs (ADR-133). Telegram
//! proves no address, so the link starts and ends in their own session:
//! they ask for a code (kept as its digest only, fifteen minutes, their
//! former one replaced) and open the bot's link carrying it in Telegram,
//! which sends it back from their private chat with the bot (`updates`);
//! the bot tells whoever opened it whose account it links, and they accept
//! it there (`answers`); their session then shows whom from, and they
//! confirm. A code someone else saw links nothing without the session that
//! asked for it, and no one's Telegram account is linked to another's
//! account unless they accepted it knowing whose. Talking with the agent
//! (`agent:use`) is required; a paused bot links nothing. A link confirmed
//! replaces the person's former one there, never this Telegram account's
//! link to anyone else, which only that person removes; audited, never what
//! Telegram showed of them.

use chrono::{DateTime, Duration, Utc};
use mimorya_core::DomainError;
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::telegram::{deep_link, link_code_hash};
use mimorya_db::is_unique_violation;
use mimorya_db::repositories::account_links::lock_active_account;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_identities::{NewGatewayIdentity, put_gateway_identity};
use mimorya_db::repositories::gateway_link_codes::{
    LinkCodeRow, delete_link_code, link_code_of, put_link_code, take_claim,
};
use mimorya_db::repositories::gateways::{GatewayRow, get_gateway};
use serde_json::json;
use uuid::Uuid;

use crate::auth::credentials::new_session_token;
use crate::services::AccessContext;
use crate::services::gateways::GatewayService;

/// How long a code stays valid.
pub const LINK_CODE_LIFETIME: Duration = Duration::minutes(15);
/// What linking requires: talking with the agent.
const AGENT_USE: &str = "agent:use";

const NOT_FOUND: &str = "Passerelle introuvable";
const UNAVAILABLE: &str = "Le bot Telegram de votre entreprise est désactivé : rien ne peut être \
     relié pour l’instant";
const NOTHING_TO_CONFIRM: &str = "Aucune liaison à confirmer : ouvrez d’abord le lien dans \
     Telegram, démarrez le bot et acceptez la liaison, puis revenez ici";
const LINKED_ELSEWHERE: &str = "Ce compte Telegram est déjà relié à une autre personne : elle \
     seule peut le retirer, depuis sa page « Mon compte »";

/// The link a person opens in Telegram to send their code back, and until
/// when it is valid.
pub struct TelegramLinkCode {
    pub link: String,
    pub expires_at: DateTime<Utc>,
}

/// Never the link, which carries the code.
impl std::fmt::Debug for TelegramLinkCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramLinkCode")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// A code the person asked for, and whom Telegram sent it back from.
#[derive(Debug, Clone)]
pub struct PendingLink {
    pub expires_at: DateTime<Utc>,
    /// How Telegram showed them, once they accepted the link.
    pub claimed_by: Option<String>,
    /// Their Telegram identifier, which no one chooses.
    pub claimed_id: Option<String>,
}

impl From<LinkCodeRow> for PendingLink {
    fn from(row: LinkCodeRow) -> Self {
        Self {
            expires_at: row.expires_at,
            claimed_by: row.claimed_name,
            claimed_id: row.claimed_external_id,
        }
    }
}

impl GatewayService {
    /// A new code for the person of `access` on the Telegram gateway
    /// `gateway_id`, and the bot's link carrying it.
    pub async fn ask_telegram_code(
        &self,
        access: &AccessContext,
        gateway_id: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<TelegramLinkCode> {
        access.require(AGENT_USE)?;
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        let gateway = usable(get_gateway(&mut tx, gateway_id).await?)?;
        let (code, hash) = new_link_code();
        let expires_at = now + LINK_CODE_LIFETIME;
        let who = (tenant_id, gateway.id, access.subject.user_id);
        put_link_code(&mut tx, who, &hash, expires_at).await?;
        tx.commit().await?;
        Ok(TelegramLinkCode {
            link: deep_link(&gateway.workspace_name, &code),
            expires_at,
        })
    }

    /// The person's code on the gateway, if unexpired at `now`.
    pub(in crate::services::gateways) async fn pending_link(
        &self,
        tx: &mut mimorya_db::TenantTx,
        gateway: &GatewayRow,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Option<PendingLink>> {
        if gateway.kind != GatewayKind::Telegram.as_str() {
            return Ok(None);
        }
        let pending = link_code_of(tx, gateway.id, user_id, now).await?;
        Ok(pending.map(PendingLink::from))
    }

    /// Links the Telegram account that sent back the person's code on the
    /// gateway `gateway_id`, unexpired at `now`.
    pub async fn confirm_telegram_link(
        &self,
        access: &AccessContext,
        gateway_id: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        access.require(AGENT_USE)?;
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        if !lock_active_account(&mut tx, access.subject.user_id).await? {
            return Err(DomainError::forbidden("Compte désactivé").into());
        }
        let gateway = usable(get_gateway(&mut tx, gateway_id).await?)?;
        let Some(claim) = take_claim(&mut tx, gateway.id, access.subject.user_id, now).await?
        else {
            return Err(DomainError::validation(NOTHING_TO_CONFIRM).into());
        };
        let replaced = match put_gateway_identity(
            &mut tx,
            NewGatewayIdentity {
                tenant_id,
                gateway_id: gateway.id,
                user_id: access.subject.user_id,
                external_id: &claim.external_id,
                address: None,
                display_name: Some(&claim.name),
            },
        )
        .await
        {
            Ok(replaced) => replaced,
            // Linked to someone else by a confirmation under way.
            Err(error) if is_unique_violation(&error) => {
                return Err(DomainError::conflict(LINKED_ELSEWHERE).into());
            }
            Err(error) => return Err(error.into()),
        };
        if replaced.other_person.is_some() {
            // Linked to someone else meanwhile: only they remove it.
            return Err(DomainError::conflict(LINKED_ELSEWHERE).into());
        }
        let entry = AuditEntry::new(tenant_id, "user", "gateway.identity_linked", "success")
            .by_user(access.subject.user_id)
            .resource("gateway", gateway.id)
            .metadata(json!({
                "externalId": claim.external_id,
                "replacedOwn": replaced.own.is_some(),
            }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Forgets the person's code on the gateway `gateway_id`, sent back or
    /// not.
    pub async fn cancel_telegram_code(
        &self,
        access: &AccessContext,
        gateway_id: Uuid,
    ) -> anyhow::Result<()> {
        let mut tx = self.db.begin(access.subject.tenant_id).await?;
        let deleted = delete_link_code(&mut tx, gateway_id, access.subject.user_id).await?;
        tx.commit().await?;
        if !deleted {
            return Err(DomainError::not_found("Aucune liaison en cours sur ce bot").into());
        }
        Ok(())
    }
}

/// A new code, as the platform writes them (`is_link_code`), and what is
/// kept of it.
fn new_link_code() -> (String, String) {
    let (code, _) = new_session_token();
    let hash = link_code_hash(&code);
    (code, hash)
}

/// The gateway, if it is a Telegram bot that may link someone.
fn usable(gateway: Option<GatewayRow>) -> Result<GatewayRow, DomainError> {
    match gateway {
        Some(gateway) if gateway.kind == GatewayKind::Telegram.as_str() => {
            if gateway.enabled {
                Ok(gateway)
            } else {
                Err(DomainError::conflict(UNAVAILABLE))
            }
        }
        _ => Err(DomainError::not_found(NOT_FOUND)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mimorya_core::gateways::telegram::is_link_code;

    #[test]
    fn a_code_is_as_the_platform_writes_them_and_kept_as_its_digest() {
        let (code, hash) = new_link_code();
        assert!(is_link_code(&code), "{code}");
        assert_eq!(hash, link_code_hash(&code));
        assert_ne!(new_link_code().0, code);
    }
}
