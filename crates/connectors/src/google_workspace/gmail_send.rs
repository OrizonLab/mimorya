//! Tool `google_gmail_send` (ADR-123): sends, from the Gmail of the person
//! the run serves and with their own rights, the e-mail they validated:
//! its recipients, subject and text, exactly as shown to them, kept in
//! their sent mail. Sent once, never repeated; always validated, whatever
//! the company's policies.

use async_trait::async_trait;
use mimorya_core::account_links::GMAIL_SEND;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use serde_json::{Value, json};

use super::message::{MESSAGE_ID_RE, link};
use super::outgoing::raw_message;
use crate::delegated::shape::bounded;
use crate::delegated::{NewMail, ProviderApi, write_as_person};
use crate::outbound::CONNECTOR_USE;

pub struct GmailSend {
    pub gmail: ProviderApi,
}

#[async_trait]
impl TypedTool for GmailSend {
    type Input = NewMail;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "google_gmail_send",
            "Envoi d’un e-mail Gmail",
            "Envoie un e-mail depuis la boîte Gmail de la personne servie, en son nom et avec ses propres droits : destinataires, objet et texte brut, exactement tels qu’ils sont validés ; le message est gardé dans ses messages envoyés.",
            ToolRisk::External,
        )
        .requires([CONNECTOR_USE])
        .always_requires_approval()
    }

    fn describe_action(&self, input: &NewMail) -> Option<String> {
        Some(input.described(self.gmail.kind().product()))
    }

    async fn run(&self, input: NewMail, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let Some(raw) = raw_message(&input) else {
            return Ok(ToolResult::error("Adresse électronique invalide"));
        };
        let message = json!({ "raw": raw });
        let url = self
            .gmail
            .address(&["users", "me", "messages", "send"], &[]);
        let sent = match write_as_person(&self.gmail, ctx, &[GMAIL_SEND], url, &message, None).await
        {
            Ok(sent) => sent.unwrap_or_default(),
            Err(why) => return Ok(ToolResult::error(why)),
        };
        // Sent, whether Gmail names the message or not; linked and cited
        // only by an identifier of Gmail's form.
        let id = sent
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| MESSAGE_ID_RE.is_match(id));
        let citations = id
            .map(|id| Citation::new("email", id).labelled(bounded(&input.subject, 120).0))
            .into_iter()
            .collect();
        let content = json!({
            "envoye": true,
            "id": id,
            "objet": input.subject,
            "a": input.to,
            "copie": input.cc,
            "lien": id.map(link),
        });
        Ok(ToolResult::ok(content)
            .with_summary(format!(
                "E-mail envoyé à {} destinataire(s)",
                input.recipients()
            ))
            .with_citations(citations))
    }
}
