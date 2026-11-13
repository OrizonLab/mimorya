//! Tool `mail_send` (ADR-205): sends, from the company's standard mail and
//! with the account the person the run serves linked, the e-mail they
//! validated: its recipients, subject and text, exactly as shown to them,
//! from the address of their linked mailbox. Sent once, never repeated;
//! always validated, whatever the company's policies; offered only where
//! the company lets its tools write. It is not copied into their sent
//! e-mails (no IMAP `APPEND`): many servers keep it themselves, and it would
//! be kept twice.

use async_trait::async_trait;
use chrono::Utc;
use lettre::Address;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{
    MailAccount, MailServerRole, ToolContext, ToolInfo, ToolResult, TypedTool,
};
use serde_json::json;
use tokio::time::timeout;
use uuid::Uuid;

use super::person::{MailReach, PersonAccount, refused, told, told_plain};
use super::submission::{Handed, Unsent, submit};
use crate::delegated::{NewMail, is_plain_address, message_text};
use crate::outbound::{CONNECTOR_USE, TOO_SLOW};
use crate::standard_mail::{CREDENTIALS_REFUSED, LEAVE_TIMEOUT, smtp_connection};

pub struct MailSend {
    pub reach: MailReach,
}

/// An e-mail ready to be handed: its envelope and its text.
struct Outgoing {
    from: Address,
    to: Vec<Address>,
    text: String,
}

#[async_trait]
impl TypedTool for MailSend {
    type Input = NewMail;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "mail_send",
            "Envoi d’un e-mail (messagerie de l’entreprise)",
            "Envoie un e-mail depuis la messagerie de l’entreprise, au nom de la personne servie et avec le compte qu’elle a relié : destinataires, objet et texte brut, exactement tels qu’ils sont validés. Il n’est copié dans ses messages envoyés que si le serveur le fait lui-même.",
            ToolRisk::External,
        )
        .requires([CONNECTOR_USE])
        .always_requires_approval()
    }

    fn describe_action(&self, input: &NewMail) -> Option<String> {
        Some(input.described("de messagerie"))
    }

    async fn run(&self, input: NewMail, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let handed = Handed::default();
        let sending = self.send(&input, ctx, &handed);
        let sent = timeout(self.reach.call_timeout, sending)
            .await
            .unwrap_or_else(|_| {
                let said = told_plain(TOO_SLOW, "");
                Err(match handed.get() {
                    true => perhaps_sent(&said),
                    false => nothing_sent(&said),
                })
            });
        let from = match sent {
            Ok(from) => from,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let content = json!({
            "envoye": true,
            "de": from,
            "objet": input.subject,
            "a": input.to,
            "copie": input.cc,
        });
        Ok(ToolResult::ok(content).with_summary(format!(
            "E-mail envoyé à {} destinataire(s)",
            input.recipients()
        )))
    }
}

impl MailSend {
    /// Sends `mail` as the run's person: the address it left from, or why
    /// it is not known to have left, in French.
    async fn send(
        &self,
        mail: &NewMail,
        ctx: &ToolContext,
        handed: &Handed,
    ) -> Result<String, String> {
        let PersonAccount {
            account,
            config,
            login,
        } = self.reach.account(ctx).await?;
        let Some(from) = sender(&account) else {
            return Err(
                "Le compte relié ne donne aucune adresse d’expéditeur : la personne le \
                        relie de nouveau dans « Mon compte » avec son adresse électronique"
                    .into(),
            );
        };
        let Some(outgoing) = outgoing(mail, from) else {
            return Err("Adresse électronique invalide".into());
        };
        let connected = smtp_connection(&config.smtp, &login, self.reach.network).await;
        let (mut connection, budget) = match connected {
            Ok(connected) => connected,
            Err(cause) if cause.summary == CREDENTIALS_REFUSED => {
                return Err(refused(ctx, MailServerRole::Smtp, account.linked_at).await);
            }
            Err(cause) => return Err(nothing_sent(&told(&login, cause))),
        };
        let envelope = (&outgoing.from, outgoing.to.as_slice());
        let text = outgoing.text.as_bytes();
        match submit(&mut connection, &budget, envelope, text, handed).await {
            Ok(()) => {
                // Sent: a goodbye the server answers badly changes nothing.
                let _ = timeout(LEAVE_TIMEOUT, connection.quit()).await;
                Ok(from.to_string())
            }
            Err(unsent) => {
                let _ = timeout(LEAVE_TIMEOUT, connection.abort()).await;
                Err(match unsent {
                    Unsent::Nothing(cause) => nothing_sent(&told(&login, cause)),
                    Unsent::Unsure(cause) => perhaps_sent(&told(&login, cause)),
                })
            }
        }
    }
}

/// The address the person sends from: the identifier of their linked
/// mailbox when it is an address, which their server checked with its
/// password; else their platform account's.
fn sender(account: &MailAccount) -> Option<&str> {
    [account.username.as_str(), account.address.as_str()]
        .into_iter()
        .find(|address| is_plain_address(address))
}

/// `mail` from `from`, its text written by the platform with the sender's
/// headers, and its envelope: each recipient once.
fn outgoing(mail: &NewMail, from: &str) -> Option<Outgoing> {
    let (_, domain) = from.rsplit_once('@')?;
    let date = Utc::now().to_rfc2822();
    let message_id = format!("<{}@{domain}>", Uuid::new_v4().simple());
    let headers = [("From", from), ("Date", &date), ("Message-ID", &message_id)];
    let text = message_text(mail, &headers)?;
    let mut to: Vec<Address> = Vec::new();
    for address in mail.to.iter().chain(&mail.cc) {
        let address: Address = address.parse().ok()?;
        let given = |known: &Address| {
            let known: &str = known.as_ref();
            known.eq_ignore_ascii_case(address.as_ref())
        };
        if !to.iter().any(given) {
            to.push(address);
        }
    }
    Some(Outgoing {
        from: from.parse().ok()?,
        to,
        text,
    })
}

/// A failure that left nothing sent, as the model reads it.
fn nothing_sent(said: &str) -> String {
    format!("{said} — rien n’est parti")
}

/// A failure once the e-mail was handed, as the model reads it.
fn perhaps_sent(said: &str) -> String {
    format!(
        "{said} — l’e-mail a été transmis sans que le serveur confirme l’avoir pris : il est \
         peut-être parti, ne le renvoyez pas sans vérifier auprès de ses destinataires"
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn account(username: &str, address: &str) -> MailAccount {
        MailAccount::new(
            json!({}),
            address.into(),
            username.into(),
            "secret".into(),
            Utc::now(),
        )
    }

    #[test]
    fn sends_from_the_linked_mailbox_or_else_the_platform_account() {
        let linked = account("lea.martin@exemple.fr", "lea@exemple.fr");
        assert_eq!(sender(&linked), Some("lea.martin@exemple.fr"));
        let named = account("lmartin", "lea@exemple.fr");
        assert_eq!(sender(&named), Some("lea@exemple.fr"));
        let neither = account("lmartin", "lea@[127.0.0.1]");
        assert_eq!(sender(&neither), None);
    }

    #[test]
    fn writes_each_recipient_once_and_the_sender_s_headers() {
        let mail: NewMail = serde_json::from_value(json!({
            "to": ["paul@exemple.fr", "anne@exemple.fr"],
            "cc": ["PAUL@exemple.fr"],
            "subject": "Point",
            "body": "Bonjour",
        }))
        .unwrap();
        let outgoing = outgoing(&mail, "lea@exemple.fr").unwrap();
        let to: Vec<&str> = outgoing.to.iter().map(|to| to.as_ref()).collect();
        assert_eq!(to, ["paul@exemple.fr", "anne@exemple.fr"]);
        assert_eq!(outgoing.from.to_string(), "lea@exemple.fr");
        let text = outgoing.text;
        assert!(text.starts_with("From: lea@exemple.fr\r\nDate: "), "{text}");
        let id = text.lines().find(|line| line.starts_with("Message-ID: <"));
        assert!(id.is_some_and(|id| id.ends_with("@exemple.fr>")), "{text}");
    }
}
