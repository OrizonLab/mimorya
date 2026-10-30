//! A person links their identity on a messaging service to their account
//! from a link the company's bot sends them there: in Slack (ADR-125), in
//! Microsoft Teams (ADR-138). Writing to the bot before they did, they are
//! sent, in their own conversation with it, a link to their account's
//! page: it carries, in its fragment (never sent to a server with the
//! page), the gateway and their identity on the service (in Teams, with
//! the service and conversation the bot met them in), sealed for their
//! company by the server's master key, valid fifteen minutes; nothing is
//! stored. Opened in their own session, the link is confirmed once the
//! service, asked again then, says that identity has the account's own
//! address in the company's workspace or directory (`slack`, `teams`): a
//! link opened by someone else links nothing. A new link replaces the
//! person's former identity there, and this identity's link to anyone
//! else. Each outcome is audited, never an address.

mod slack;
mod teams;

use chrono::{DateTime, Duration, Utc};
use mimorya_core::DomainError;
use mimorya_core::gateways::GatewayKind;
use mimorya_db::repositories::account_links::lock_active_account;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_identities::{NewGatewayIdentity, put_gateway_identity};
use mimorya_db::repositories::gateways::{GatewayRow, get_gateway};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{GatewayService, LINK_PAGE, bot_token_name, client_secret_name};
use crate::services::AccessContext;
use crate::services::sso::{open_value, seal_value};

/// How long a link a bot gave stays valid.
pub const LINK_LIFETIME: Duration = Duration::minutes(15);
/// The associated data's name: never another sealed value's.
const SEALED_AS: &str = "gateway.link";
/// What linking requires: talking with the agent.
const AGENT_USE: &str = "agent:use";

const INVALID: &str = "Lien de liaison invalide : ouvrez celui que le bot de votre entreprise \
     vous a envoyé dans votre messagerie";
const EXPIRED: &str = "Ce lien a expiré : écrivez de nouveau au bot de votre entreprise dans \
     votre messagerie pour en recevoir un autre";

/// What a link carries, sealed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LinkCode {
    gateway_id: Uuid,
    /// Their identity on the service: a Slack user, a Teams user's Entra
    /// object.
    person: String,
    expires_at: DateTime<Utc>,
    /// Where the Teams bot met them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    teams: Option<TeamsReference>,
}

/// Where a Teams bot met a person: the service and conversation it may ask
/// who they are in, and their member identifier there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TeamsReference {
    service_url: String,
    conversation_id: String,
    member_id: String,
}

/// What a link would link, shown before it is confirmed.
#[derive(Debug, Clone)]
pub struct LinkPreview {
    pub gateway: GatewayRow,
    pub external_id: String,
    pub expires_at: DateTime<Utc>,
}

/// Why the service does not take an identity for the person's own.
struct Refused {
    code: &'static str,
    message: &'static str,
}

impl GatewayService {
    /// The address of the account page carrying `code` sealed for the
    /// company `tenant_id`; `None` on a server without a master key.
    fn link_url(&self, tenant_id: Uuid, code: &LinkCode) -> anyhow::Result<Option<String>> {
        let Some(cipher) = self.cipher.as_deref() else {
            return Ok(None);
        };
        let sealed = seal_value(cipher, tenant_id, SEALED_AS, code)?;
        // In the fragment: never sent to a server with the page.
        Ok(Some(format!("{}{LINK_PAGE}#{sealed}", self.web_origin)))
    }

    /// What the link `code` would link for the person of `access`.
    pub async fn preview_link(
        &self,
        access: &AccessContext,
        code: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<LinkPreview> {
        let code = self.opened(access, code, now)?;
        let mut tx = self.db.begin(access.subject.tenant_id).await?;
        let gateway = usable(get_gateway(&mut tx, code.gateway_id).await?)?;
        fitting(&gateway, &code)?;
        Ok(LinkPreview {
            gateway,
            external_id: code.person,
            expires_at: code.expires_at,
        })
    }

    /// Links the identity the link `code` carries to the person of
    /// `access`, once its service proves it is theirs.
    pub async fn confirm_link(
        &self,
        access: &AccessContext,
        code: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        access.require(AGENT_USE)?;
        let code = self.opened(access, code, now)?;
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        let gateway = usable(get_gateway(&mut tx, code.gateway_id).await?)?;
        let kind = fitting(&gateway, &code)?;
        let credential = match kind {
            GatewayKind::Teams => client_secret_name(gateway.id),
            _ => bot_token_name(gateway.id),
        };
        let credential = self.secret(&mut tx, tenant_id, &credential).await?;
        tx.commit().await?;
        let Some(credential) = credential else {
            return Err(DomainError::unavailable(format!(
                "Le moyen d’authentification du bot {} de l’entreprise ne peut pas être lu sur \
                 ce serveur : votre administrateur doit le saisir de nouveau",
                kind.product()
            ))
            .into());
        };
        let email = &access.profile.email;
        // Asked of the service now, not when the link was sent: who that
        // identity is today.
        let proven = match kind {
            GatewayKind::Teams => {
                self.teams_proof(&gateway, &credential, &code, email)
                    .await?
            }
            _ => {
                self.slack_proof(&gateway, &credential, &code, email)
                    .await?
            }
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let address = match proven {
            Ok(address) => address,
            Err(refused) => {
                let entry = audited(access, "gateway.identity_refused", "failure", gateway.id)
                    .metadata(json!({ "reason": refused.code }));
                append_audit(&mut tx, entry).await?;
                tx.commit().await?;
                return Err(DomainError::validation(refused.message).into());
            }
        };
        if !lock_active_account(&mut tx, access.subject.user_id).await? {
            return Err(DomainError::forbidden("Compte désactivé").into());
        }
        // Deleted or paused meanwhile: nothing is linked.
        let gateway = usable(get_gateway(&mut tx, gateway.id).await?)?;
        let replaced = put_gateway_identity(
            &mut tx,
            NewGatewayIdentity {
                tenant_id,
                gateway_id: gateway.id,
                user_id: access.subject.user_id,
                external_id: &code.person,
                address: Some(address.trim()),
                display_name: None,
            },
        )
        .await?;
        let entry =
            audited(access, "gateway.identity_linked", "success", gateway.id).metadata(json!({
                "externalId": code.person,
                "replacedOwn": replaced.own.is_some(),
                "replacedOtherPerson": replaced.other_person.is_some(),
            }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(())
    }

    /// The link `code`, opened for the person's own company, still valid.
    fn opened(
        &self,
        access: &AccessContext,
        code: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<LinkCode> {
        let cipher = self
            .cipher
            .as_deref()
            .ok_or_else(|| DomainError::not_found(INVALID))?;
        let opened = open_value::<LinkCode>(cipher, code.trim(), SEALED_AS)
            .filter(|(tenant_id, _)| *tenant_id == access.subject.tenant_id)
            .map(|(_, code)| code);
        let Some(code) = opened else {
            return Err(DomainError::not_found(INVALID).into());
        };
        if code.expires_at <= now {
            return Err(DomainError::validation(EXPIRED).into());
        }
        Ok(code)
    }
}

/// The gateway's service, if `code` is a link its bot sends: a Slack
/// user's for a Slack application, a Teams user's, met in a conversation,
/// for a Teams bot.
fn fitting(gateway: &GatewayRow, code: &LinkCode) -> Result<GatewayKind, DomainError> {
    let kind = GatewayKind::of(&gateway.kind);
    let fits = match (kind, &code.teams) {
        (Some(GatewayKind::Slack), None) => mimorya_core::gateways::slack::is_person(&code.person),
        (Some(GatewayKind::Teams), Some(_)) => {
            mimorya_core::gateways::teams::is_person(&code.person)
        }
        _ => false,
    };
    match kind {
        Some(kind) if fits => Ok(kind),
        _ => Err(DomainError::not_found(INVALID)),
    }
}

/// The gateway, if it is there and enabled.
fn usable(gateway: Option<GatewayRow>) -> Result<GatewayRow, DomainError> {
    match gateway {
        None => Err(DomainError::not_found(INVALID)),
        Some(gateway) if !gateway.enabled => {
            let product = GatewayKind::of(&gateway.kind).map_or("", GatewayKind::product);
            Err(DomainError::conflict(format!(
                "La passerelle {product} de votre entreprise est désactivée : rien ne peut être \
                 relié pour l’instant"
            )))
        }
        Some(gateway) => Ok(gateway),
    }
}

fn audited(
    access: &AccessContext,
    action: &'static str,
    result: &'static str,
    gateway_id: Uuid,
) -> AuditEntry {
    AuditEntry::new(access.subject.tenant_id, "user", action, result)
        .by_user(access.subject.user_id)
        .resource("gateway", gateway_id)
}
