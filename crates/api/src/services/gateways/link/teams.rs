//! The link a Teams user is sent (ADR-138), in the personal conversation
//! with the bot they wrote in, with the service and conversation sealed in
//! it; and its proof: Teams, asked through that conversation when the link
//! is confirmed, says who its member is now (`identity_refusal`): the
//! Entra object the link was sent to, of the company's directory, with the
//! account's own address.

use chrono::{DateTime, Utc};
use mimorya_connectors::teams::TeamsFailure;
use mimorya_core::DomainError;
use mimorya_core::gateways::teams::{
    PersonalMessage, TeamsApplication, TeamsMember, escaped, identity_refusal, link,
};
use mimorya_db::repositories::gateways::GatewayRow;
use uuid::Uuid;

use super::{INVALID, LINK_LIFETIME, LinkCode, Refused, TeamsReference};
use crate::services::gateways::GatewayService;

impl GatewayService {
    /// Sends `message`'s author, in the conversation they wrote in, the
    /// link to their account's page; in the background, once the activity
    /// is answered.
    pub(in crate::services::gateways) fn offer_teams_link(
        &self,
        (tenant_id, gateway_id): (Uuid, Uuid),
        app: TeamsApplication,
        message: &PersonalMessage,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let code = LinkCode {
            gateway_id,
            person: message.person.aad_object_id.clone(),
            expires_at: now + LINK_LIFETIME,
            teams: Some(TeamsReference {
                service_url: message.service_url.clone(),
                conversation_id: message.conversation_id.clone(),
                member_id: message.person.id.clone(),
            }),
        };
        let Some(url) = self.link_url(tenant_id, &code)? else {
            return Ok(());
        };
        let Some(text) = link_text(&self.assistant_name, &url) else {
            tracing::warn!(gateway = %gateway_id, "Teams link not written: unusual address");
            return Ok(());
        };
        let conversation = (message.service_url.clone(), message.conversation_id.clone());
        self.say_in_teams(gateway_id, app, conversation, text);
        Ok(())
    }

    /// The address Teams gives as the link's member's, if Teams, asked as
    /// the bot with its `client_secret`, says they are the Entra object the
    /// link was sent to, of the company's directory, with the account's own
    /// address `email`; why not otherwise.
    pub(super) async fn teams_proof(
        &self,
        gateway: &GatewayRow,
        client_secret: &str,
        code: &LinkCode,
        email: &str,
    ) -> anyhow::Result<Result<String, Refused>> {
        let Some(reference) = &code.teams else {
            return Err(DomainError::not_found(INVALID).into());
        };
        let app = TeamsApplication {
            app_id: gateway.bot_user_id.clone(),
            directory_id: gateway.workspace_id.clone(),
            client_secret: client_secret.to_owned(),
        };
        let conversation = (
            reference.service_url.as_str(),
            reference.conversation_id.as_str(),
        );
        let member = self
            .teams
            .member(&app, conversation, &reference.member_id)
            .await
            .map_err(|failure| DomainError::unavailable(refused_to_person(&failure)))?;
        let profile = TeamsMember {
            aad_object_id: member.aad_object_id.as_deref(),
            tenant_id: member.tenant_id.as_deref(),
            email: member.email.as_deref(),
            user_principal_name: member.user_principal_name.as_deref(),
        };
        let expected = (code.person.as_str(), gateway.workspace_id.as_str());
        Ok(match identity_refusal(&profile, expected, email) {
            Some(refusal) => Err(Refused {
                code: refusal.code(),
                message: refusal.message(),
            }),
            // The address Teams gave that is the account's.
            None => Ok([member.email, member.user_principal_name]
                .into_iter()
                .flatten()
                .find(|address| address.trim().eq_ignore_ascii_case(email.trim()))
                .unwrap_or_else(|| email.to_owned())),
        })
    }
}

/// What the person is told when Teams does not say who they are: the bot
/// itself refused is their administrator's to fix.
fn refused_to_person(failure: &TeamsFailure) -> String {
    if failure.credentials_refused() {
        return "Microsoft refuse de dire au bot de votre entreprise qui est ce compte : \
             prévenez votre administrateur (identifiant, annuaire ou secret client du bot \
             Teams à revoir)"
            .into();
    }
    failure.message()
}

/// The message offering the link, in the Markdown Teams reads: the
/// assistant's name escaped, the link's address made only of the web
/// origin, the page and the sealed code; `None` if that address holds
/// anything else.
fn link_text(assistant_name: &str, url: &str) -> Option<String> {
    let link = link(url, "relier mon compte")?;
    Some(format!(
        "Pour échanger ici avec {}, reliez votre compte Teams à votre compte : {link}. Le lien \
         est valable 15 minutes ; ouvrez-le dans le navigateur où vous êtes connecté. L’adresse \
         de votre compte Microsoft doit être celle de votre compte.",
        escaped(assistant_name)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_message_quotes_nothing_teams_would_read_as_markup() {
        let text = link_text(
            "Assistant <b>[co](x)</b>",
            "https://app.example/account/gateway-link#a.b",
        )
        .unwrap();
        assert!(
            text.contains("Assistant &lt;b&gt;\\[co\\]\\(x\\)&lt;/b&gt;"),
            "{text}"
        );
        assert!(
            text.contains("[relier mon compte](https://app.example/account/gateway-link#a.b)"),
            "{text}"
        );
        assert_eq!(link_text("A", "https://app.example/a b"), None);
    }
}
