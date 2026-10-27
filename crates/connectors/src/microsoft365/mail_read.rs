//! Tool `m365_mail_read` (ADR-117): one e-mail of the person the run
//! serves, its text (Graph converts it from HTML), cut past a bound, its
//! recipients and whether it has attachments, which are not read.

use std::sync::LazyLock;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use validator::Validate;

use super::graph::Graph;
use super::mail_search::citation;
use super::reach::read_as_person;
use super::shape::{bounded, people, person, short};
use crate::outbound::CONNECTOR_USE;

const SELECT: &str = "id,subject,from,toRecipients,ccRecipients,receivedDateTime,body,\
     hasAttachments,webLink";
/// Graph gives the text of an HTML e-mail.
const AS_TEXT: &str = "outlook.body-content-type=\"text\"";
/// The most of an e-mail's text read.
pub const MAX_BODY_CHARS: usize = 20_000;
const MAX_RECIPIENTS: usize = 20;

/// Graph's identifiers: base 64, possibly URL-safe.
static MESSAGE_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_=+/-]{1,512}$").expect("valid regex"));

pub struct MailRead {
    pub graph: Graph,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MailReadInput {
    /// Identifiant de l’e-mail, donné par m365_mail_search
    #[schemars(regex(pattern = r"^[A-Za-z0-9_=+/-]{1,512}$"))]
    #[validate(regex(path = *MESSAGE_ID_RE, message = "Identifiant d’e-mail invalide"))]
    id: String,
}

fn recipients(message: &Value, field: &str) -> Value {
    let (kept, more) = people(message.get(field), MAX_RECIPIENTS);
    if more == 0 {
        json!(kept)
    } else {
        json!({ "personnes": kept, "autres": more })
    }
}

#[async_trait]
impl TypedTool for MailRead {
    type Input = MailReadInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "m365_mail_read",
            "Lecture d’un e-mail Microsoft 365",
            "Renvoie un e-mail de la boîte Microsoft 365 de la personne servie : expéditeur, destinataires, date et texte (tronqué au-delà de 20 000 caractères). Les pièces jointes sont signalées, pas lues.",
            ToolRisk::Read,
        )
        .requires([CONNECTOR_USE])
    }

    async fn run(&self, input: MailReadInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let url = self.graph.address(
            &["me", "messages", &input.id],
            &[("$select", SELECT.to_string())],
        );
        let message = match read_as_person(&self.graph, ctx, url, Some(AS_TEXT)).await {
            Ok(message) => message,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let body = message
            .pointer("/body/content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let (text, cut) = bounded(body.trim(), MAX_BODY_CHARS);
        let content = json!({
            "id": message.get("id"),
            "objet": short(&message, "/subject"),
            "de": message.get("from").map_or(Value::Null, person),
            "a": recipients(&message, "toRecipients"),
            "cc": recipients(&message, "ccRecipients"),
            "recu": short(&message, "/receivedDateTime"),
            "texte": text,
            "texteTronque": cut,
            "piecesJointes": message.get("hasAttachments"),
            "lien": short(&message, "/webLink"),
        });
        // The subject stays out of the journal: the citation carries it.
        Ok(ToolResult::ok(content)
            .with_summary("E-mail lu")
            .with_citations(citation(&message).into_iter().collect()))
    }
}
