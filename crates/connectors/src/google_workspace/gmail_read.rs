//! Tool `google_gmail_read` (ADR-122): one e-mail of the person the run
//! serves, its text (the plain text part, else the HTML one as text, in
//! the charset it declares), cut past a bound, its senders and recipients
//! as Gmail gives them, and the names of its attachments, which are not
//! read.

use std::sync::LazyLock;

use async_trait::async_trait;
use mimorya_core::account_links::GMAIL_READONLY;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::Validate;

use super::message::{citation, header_of, link, received};
use super::mime::read;
use crate::delegated::shape::{MAX_SHORT_CHARS, bounded};
use crate::delegated::{ProviderApi, read_as_person};
use crate::outbound::CONNECTOR_USE;

/// The most of an e-mail's text read.
pub const MAX_BODY_CHARS: usize = 20_000;
/// The most of a list of recipients kept.
const MAX_RECIPIENTS_CHARS: usize = 2_000;

/// Gmail's identifiers: hexadecimal, URL-safe at most.
static MESSAGE_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{1,256}$").expect("valid regex"));

pub struct GmailRead {
    pub gmail: ProviderApi,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct GmailReadInput {
    /// Identifiant de l’e-mail, donné par google_gmail_search
    #[schemars(regex(pattern = r"^[A-Za-z0-9_-]{1,256}$"))]
    #[validate(regex(path = *MESSAGE_ID_RE, message = "Identifiant d’e-mail invalide"))]
    id: String,
}

#[async_trait]
impl TypedTool for GmailRead {
    type Input = GmailReadInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "google_gmail_read",
            "Lecture d’un e-mail Gmail",
            "Renvoie un e-mail de la boîte Gmail de la personne servie : expéditeur, destinataires, date et texte (tronqué au-delà de 20 000 caractères). Les pièces jointes sont nommées, pas lues.",
            ToolRisk::Read,
        )
        .requires([CONNECTOR_USE])
    }

    async fn run(&self, input: GmailReadInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let url = self.gmail.address(
            &["users", "me", "messages", &input.id],
            &[("format", "full".into())],
        );
        let message = match read_as_person(&self.gmail, ctx, &[GMAIL_READONLY], url, None).await {
            Ok(message) => message,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let mail = message.get("payload").map(read).unwrap_or_default();
        let (text, cut) = bounded(&mail.text, MAX_BODY_CHARS);
        let mut content = json!({
            "id": input.id,
            "objet": header_of(&message, "Subject", MAX_SHORT_CHARS),
            "de": header_of(&message, "From", MAX_SHORT_CHARS),
            "a": header_of(&message, "To", MAX_RECIPIENTS_CHARS),
            "cc": header_of(&message, "Cc", MAX_RECIPIENTS_CHARS),
            "recu": received(&message),
            "texte": text,
            "texteTronque": cut,
            "piecesJointes": mail.attachments,
            "lien": link(&input.id),
        });
        if mail.more_attachments > 0 {
            content["autresPiecesJointes"] = json!(mail.more_attachments);
        }
        // The subject stays out of the journal: the citation carries it.
        Ok(ToolResult::ok(content)
            .with_summary("E-mail lu")
            .with_citations(citation(&message).into_iter().collect()))
    }
}
