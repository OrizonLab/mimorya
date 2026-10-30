//! The link a Slack user is sent (ADR-125), in their own direct
//! conversation with the application, addressed by their identifier, not
//! the conversation the event names (ADR-131); and its proof: Slack, asked
//! when it is confirmed, says that Slack user is of the company's
//! workspace and has the account's own address (`identity_refusal`).

use chrono::{DateTime, Utc};
use mimorya_connectors::slack::SlackFailure;
use mimorya_core::DomainError;
use mimorya_core::gateways::slack::{DirectMessage, SlackProfile, escaped, identity_refusal};
use mimorya_db::repositories::gateways::GatewayRow;
use uuid::Uuid;

use super::{LINK_LIFETIME, LinkCode, Refused};
use crate::services::gateways::GatewayService;

impl GatewayService {
    /// Sends `message`'s author, in their conversation with the application,
    /// the link to their account's page; in the background, once the event
    /// is answered.
    pub(in crate::services::gateways) fn offer_link(
        &self,
        tenant_id: Uuid,
        gateway_id: Uuid,
        token: String,
        message: DirectMessage,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let code = LinkCode {
            gateway_id,
            person: message.person.clone(),
            expires_at: now + LINK_LIFETIME,
            teams: None,
        };
        let Some(url) = self.link_url(tenant_id, &code)? else {
            return Ok(());
        };
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

    /// The address Slack gives as the link's Slack user's, if Slack, asked
    /// with the application's `token`, says they are of its workspace with
    /// the account's own address `email`; why not otherwise.
    pub(super) async fn slack_proof(
        &self,
        gateway: &GatewayRow,
        token: &str,
        code: &LinkCode,
        email: &str,
    ) -> anyhow::Result<Result<String, Refused>> {
        let person = self
            .slack
            .person(token, &code.person)
            .await
            .map_err(|failure| DomainError::unavailable(refused_to_person(&failure)))?;
        let profile = SlackProfile {
            workspace: person.workspace_id.as_deref(),
            address: person.address.as_deref(),
            deleted: person.deleted,
            bot: person.bot,
        };
        Ok(
            match identity_refusal(&profile, &gateway.workspace_id, email) {
                Some(refusal) => Err(Refused {
                    code: refusal.code(),
                    message: refusal.message(),
                }),
                None => Ok(person.address.unwrap_or_default()),
            },
        )
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
