//! Tool `m365_mail_send` (ADR-119): sends, from the mailbox of the person
//! the run serves and with their own rights, the e-mail they validated:
//! its recipients, subject and text, exactly as shown to them, kept in
//! their sent items. Sent once, never repeated; always validated, whatever
//! the company's policies.

use async_trait::async_trait;
use mimorya_core::account_links::MAIL_SEND;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool};
use serde_json::json;

use super::shape::{recipients, text_body};
use crate::delegated::{NewMail, ProviderApi, write_as_person};
use crate::outbound::CONNECTOR_USE;

pub struct MailSend {
    pub graph: ProviderApi,
}

#[async_trait]
impl TypedTool for MailSend {
    type Input = NewMail;

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

    fn describe_action(&self, input: &NewMail) -> Option<String> {
        Some(input.described(self.graph.kind().product()))
    }

    async fn run(&self, input: NewMail, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
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
        let count = input.recipients();
        Ok(ToolResult::ok(json!({
            "envoye": true,
            "objet": input.subject,
            "a": input.to,
            "copie": input.cc,
        }))
        .with_summary(format!("E-mail envoyé à {count} destinataire(s)")))
    }
}
