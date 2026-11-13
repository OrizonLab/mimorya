//! Tool `mail_send` (ADR-205): sends, from the company's standard mail and
//! with the account the person the run serves linked, the e-mail they
//! validated: its recipients, subject and text, exactly as shown to them,
//! from the address of their linked mailbox. Sent once, never repeated;
//! always validated, whatever the company's policies; offered only where
//! the company lets its tools write. It is not copied into their sent
//! e-mails (no IMAP `APPEND`): many servers keep it themselves, and it would
//! be kept twice. The call's delay ends once the server answered the text:
//! leaving it after does not change what the model is told.

use async_trait::async_trait;
use lettre::transport::smtp::client::AsyncSmtpConnection;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{MailServerRole, ToolContext, ToolInfo, ToolResult, TypedTool};
use serde_json::json;
use tokio::time::timeout;

use super::outgoing::{NO_SENDER, outgoing, sender};
use super::person::{MailReach, PersonAccount, refused, told, told_plain};
use super::submission::{Handed, Unsent, submit};
use crate::delegated::NewMail;
use crate::outbound::{CONNECTOR_USE, TOO_SLOW};
use crate::standard_mail::{CREDENTIALS_REFUSED, LEAVE_TIMEOUT, smtp_connection};

pub struct MailSend {
    pub reach: MailReach,
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
        let said = match timeout(self.reach.call_timeout, sending).await {
            Ok(Attempt { said, connection }) => {
                // The e-mail's fate known, the server is left outside the
                // call's delay: a late goodbye changes nothing.
                if let Some(connection) = connection {
                    leave(connection, said.is_ok()).await;
                }
                said
            }
            Err(_) => {
                let said = told_plain(TOO_SLOW, "");
                Err(match handed.get() {
                    true => perhaps_sent(&said),
                    false => nothing_sent(&said),
                })
            }
        };
        let from = match said {
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

/// An attempt to send: the address the e-mail left from, or why it is not
/// known to have left, in French; and the connection to leave, if one was
/// opened.
struct Attempt {
    said: Result<String, String>,
    connection: Option<AsyncSmtpConnection>,
}

impl Attempt {
    /// Nothing was sent, and no connection is left open.
    fn unsent(why: String) -> Self {
        Self {
            said: Err(why),
            connection: None,
        }
    }
}

impl MailSend {
    /// Sends `mail` as the run's person, leaving the connection open.
    async fn send(&self, mail: &NewMail, ctx: &ToolContext, handed: &Handed) -> Attempt {
        let PersonAccount {
            account,
            config,
            login,
        } = match self.reach.account(ctx).await {
            Ok(person) => person,
            Err(why) => return Attempt::unsent(why),
        };
        let outgoing = match sender(&account).map(|from| outgoing(mail, from)) {
            Some(Ok(outgoing)) => outgoing,
            Some(Err(why)) => return Attempt::unsent(nothing_sent(&why)),
            None => return Attempt::unsent(nothing_sent(NO_SENDER)),
        };
        let connected = smtp_connection(&config.smtp, &login, self.reach.network).await;
        let (mut connection, budget) = match connected {
            Ok(connected) => connected,
            Err(cause) if cause.summary == CREDENTIALS_REFUSED => {
                return Attempt::unsent(
                    refused(ctx, MailServerRole::Smtp, account.linked_at).await,
                );
            }
            Err(cause) => return Attempt::unsent(nothing_sent(&told(&login, cause))),
        };
        let envelope = (&outgoing.from, outgoing.to.as_slice());
        let text = outgoing.text.as_bytes();
        let said = match submit(&mut connection, &budget, envelope, text, handed).await {
            Ok(()) => Ok(outgoing.from.to_string()),
            Err(Unsent::Nothing(cause)) => Err(nothing_sent(&told(&login, cause))),
            Err(Unsent::Unsure(cause)) => Err(perhaps_sent(&told(&login, cause))),
        };
        Attempt {
            said,
            connection: Some(connection),
        }
    }
}

/// Leaves the server: `QUIT` once the e-mail was taken, the connection cut
/// otherwise; neither waits past its own short delay.
async fn leave(mut connection: AsyncSmtpConnection, sent: bool) {
    match sent {
        true => {
            let _ = timeout(LEAVE_TIMEOUT, connection.quit()).await;
        }
        false => {
            let _ = timeout(LEAVE_TIMEOUT, connection.abort()).await;
        }
    }
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
