//! A person links their Slack identity to their account (ADR-125). Writing
//! to the application before they did, they are sent, in their own direct
//! conversation with it (addressed by their identifier, not the
//! conversation the event names, ADR-131), a link to their account's page:
//! it carries, in its
//! fragment (never sent to a server with the page), the gateway and their
//! Slack user, sealed for their company by the server's master key, valid
//! fifteen minutes; nothing is stored. Opened in their
//! own session, the link is confirmed once Slack, asked again then, says
//! that Slack user is of the company's workspace and has the account's own
//! address (`identity_refusal`): a link opened by someone else links
//! nothing. A new link replaces the person's former identity there, and
//! this identity's link to anyone else. Each outcome is audited, never an
//! address.

use chrono::{DateTime, Duration, Utc};
use mimorya_connectors::slack::SlackFailure;
use mimorya_core::DomainError;
use mimorya_core::gateways::slack::{
    DirectMessage, SlackProfile, escaped, identity_refusal, is_person,
};
use mimorya_db::repositories::account_links::lock_active_account;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_identities::{NewGatewayIdentity, put_gateway_identity};
use mimorya_db::repositories::gateways::{GatewayRow, get_gateway};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{GatewayService, LINK_PAGE, bot_token_name};
use crate::services::AccessContext;
use crate::services::sso::{open_value, seal_value};

/// How long a link Slack gave stays valid.
pub const LINK_LIFETIME: Duration = Duration::minutes(15);
/// The associated data's name: never another sealed value's.
const SEALED_AS: &str = "gateway.link";
/// What linking requires: talking with the agent.
const AGENT_USE: &str = "agent:use";

const INVALID: &str = "Lien de liaison invalide : ouvrez celui que l’application vous a envoyé \
     dans Slack";
const EXPIRED: &str = "Ce lien a expiré : écrivez de nouveau à l’application dans Slack pour en \
     recevoir un autre";
const UNAVAILABLE: &str = "La passerelle Slack de votre entreprise est désactivée : rien ne peut \
     être relié pour l’instant";

/// What a link carries, sealed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LinkCode {
    gateway_id: Uuid,
    person: String,
    expires_at: DateTime<Utc>,
}

/// What a link would link, shown before it is confirmed.
#[derive(Debug, Clone)]
pub struct LinkPreview {
    pub gateway: GatewayRow,
    pub external_id: String,
    pub expires_at: DateTime<Utc>,
}

impl GatewayService {
    /// Sends `message`'s author, in their conversation with the application,
    /// the link to their account's page; in the background, once the event
    /// is answered.
    pub(super) fn offer_link(
        &self,
        tenant_id: Uuid,
        gateway_id: Uuid,
        token: String,
        message: DirectMessage,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let Some(cipher) = self.cipher.as_deref() else {
            return Ok(());
        };
        let code = LinkCode {
            gateway_id,
            person: message.person.clone(),
            expires_at: now + LINK_LIFETIME,
        };
        let sealed = seal_value(cipher, tenant_id, SEALED_AS, &code)?;
        // In the fragment: never sent to a server with the page.
        let url = format!("{}{LINK_PAGE}#{sealed}", self.web_origin);
        let text = link_text(&self.assistant_name, &url);
        let slack = self.slack.clone();
        self.background.spawn(async move {
            // In their own direct conversation, found by Slack from their
            // identifier, never the one the event names (ADR-131).
            if let Err(failure) = slack.post(&token, &message.person, None, &text).await {
                tracing::warn!(gateway = %gateway_id, failure = %failure.message(), "Slack link not sent");
            }
        });
        Ok(())
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
        Ok(LinkPreview {
            gateway,
            external_id: code.person,
            expires_at: code.expires_at,
        })
    }

    /// Links the Slack identity the link `code` carries to the person of
    /// `access`, once Slack proves it is theirs.
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
        let token = self
            .secret(&mut tx, tenant_id, &bot_token_name(gateway.id))
            .await?;
        tx.commit().await?;
        let Some(token) = token else {
            return Err(DomainError::unavailable(
                "Le jeton de l’application Slack ne peut pas être lu sur ce serveur : votre \
                 administrateur doit le saisir de nouveau",
            )
            .into());
        };
        // Asked of Slack now, not when the link was sent: who that Slack
        // user is today.
        let person = self
            .slack
            .person(&token, &code.person)
            .await
            .map_err(|failure| DomainError::unavailable(refused_to_person(&failure)))?;
        let profile = SlackProfile {
            workspace: person.workspace_id.as_deref(),
            address: person.address.as_deref(),
            deleted: person.deleted,
            bot: person.bot,
        };
        let refusal = identity_refusal(&profile, &gateway.workspace_id, &access.profile.email);
        let mut tx = self.db.begin(tenant_id).await?;
        if let Some(refusal) = refusal {
            let entry = audited(access, "gateway.identity_refused", "failure", gateway.id)
                .metadata(json!({ "reason": refusal.code() }));
            append_audit(&mut tx, entry).await?;
            tx.commit().await?;
            return Err(DomainError::validation(refusal.message()).into());
        }
        if !lock_active_account(&mut tx, access.subject.user_id).await? {
            return Err(DomainError::forbidden("Compte désactivé").into());
        }
        // Deleted or paused meanwhile: nothing is linked.
        let gateway = usable(get_gateway(&mut tx, gateway.id).await?)?;
        let address = person.address.unwrap_or_default();
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
            .map(|(_, code)| code)
            .filter(|code| is_person(&code.person));
        let Some(code) = opened else {
            return Err(DomainError::not_found(INVALID).into());
        };
        if code.expires_at <= now {
            return Err(DomainError::validation(EXPIRED).into());
        }
        Ok(code)
    }
}

/// What the person is told when Slack does not say who they are: the
/// application itself refused is their administrator's to fix.
fn refused_to_person(failure: &SlackFailure) -> String {
    match failure {
        SlackFailure::Refused(code) if failure.token_refused() || code == "missing_scope" => {
            "Slack refuse de dire à l’application de votre entreprise qui est ce compte : \
             prévenez votre administrateur (jeton ou permissions de l’application Slack à \
             revoir)"
                .into()
        }
        other => other.message(),
    }
}

/// The gateway, if it is there and enabled.
fn usable(gateway: Option<GatewayRow>) -> Result<GatewayRow, DomainError> {
    match gateway {
        None => Err(DomainError::not_found(INVALID)),
        Some(gateway) if !gateway.enabled => Err(DomainError::conflict(UNAVAILABLE)),
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

/// The message offering the link, in Slack's markup: the assistant's name
/// escaped as Slack asks (`&`, `<`, `>`), the link's address made only of
/// the web origin, the page and the sealed code.
fn link_text(assistant_name: &str, url: &str) -> String {
    format!(
        "Pour échanger ici avec {}, reliez votre compte Slack à votre compte : <{url}|relier mon \
         compte>. Le lien est valable 15 minutes ; ouvrez-le dans le navigateur où vous êtes \
         connecté. L’adresse de votre profil Slack doit être celle de votre compte.",
        escaped(assistant_name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_message_quotes_nothing_slack_would_read_as_markup() {
        let text = link_text(
            "Assistant <!channel> & co",
            "https://app.example/account/gateway-link#a.b",
        );
        assert!(text.contains("Assistant &lt;!channel&gt; &amp; co"));
        assert!(text.contains("<https://app.example/account/gateway-link#a.b|relier mon compte>"));
        assert!(!text.contains("<!channel>"));
    }
}
