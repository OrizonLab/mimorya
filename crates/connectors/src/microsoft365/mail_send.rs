//! Tool `m365_mail_send` (ADR-119): sends, from the mailbox of the person
//! the run serves and with their own rights, the e-mail they validated:
//! its recipients, subject and text, exactly as shown to them, kept in
//! their sent items. Sent once, never repeated; always validated, unless
//! the company's policy says otherwise.

use async_trait::async_trait;
use mimorya_core::account_links::MAIL_SEND;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::Validate;

use super::compose::{addresses, excerpt, full_text, recipients, text_body, visible_line};
use super::graph::Graph;
use super::reach::write_as_person;
use crate::outbound::CONNECTOR_USE;

pub struct MailSend {
    pub graph: Graph,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MailSendInput {
    /// Destinataires : adresses e-mail, 1 à 10
    #[validate(length(min = 1, max = 10), custom(function = "addresses"))]
    to: Vec<String>,
    /// Destinataires en copie : adresses e-mail, 10 au plus
    #[serde(default)]
    #[validate(length(max = 10), custom(function = "addresses"))]
    cc: Vec<String>,
    /// Objet, sur une ligne
    #[validate(length(min = 1, max = 255), custom(function = "visible_line"))]
    subject: String,
    /// Texte du message, en texte brut (retours à la ligne permis)
    #[validate(length(min = 1, max = 20000), custom(function = "full_text"))]
    body: String,
}

#[async_trait]
impl TypedTool for MailSend {
    type Input = MailSendInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "m365_mail_send",
            "Envoi d’un e-mail Microsoft 365",
            "Envoie un e-mail depuis la boîte Microsoft 365 de la personne servie, en son nom et avec ses propres droits : destinataires, objet et texte brut, exactement tels qu’ils sont validés ; le message est gardé dans ses éléments envoyés.",
            ToolRisk::External,
        )
        .requires([CONNECTOR_USE])
        .always_requires_approval()
    }

    fn describe_action(&self, input: &MailSendInput) -> Option<String> {
        let copy = match input.cc.is_empty() {
            true => String::new(),
            false => format!(" (copie : {})", input.cc.join(", ")),
        };
        Some(format!(
            "Envoyer l’e-mail « {} » depuis la boîte Microsoft 365 reliée, à {}{copy} : « {} »",
            input.subject,
            input.to.join(", "),
            excerpt(&input.body)
        ))
    }

    async fn run(&self, input: MailSendInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let message = json!({
            "message": {
                "subject": input.subject,
                "body": text_body(&input.body),
                "toRecipients": recipients(&input.to, None),
                "ccRecipients": recipients(&input.cc, None),
            },
            "saveToSentItems": true,
        });
        let url = self.graph.address(&["me", "sendMail"], &[]);
        if let Err(why) = write_as_person(&self.graph, ctx, &[MAIL_SEND], url, &message, None).await
        {
            return Ok(ToolResult::error(why));
        }
        let count = input.to.len() + input.cc.len();
        Ok(ToolResult::ok(json!({
            "envoye": true,
            "objet": input.subject,
            "a": input.to,
            "copie": input.cc,
        }))
        .with_summary(format!("E-mail envoyé à {count} destinataire(s)")))
    }
}
