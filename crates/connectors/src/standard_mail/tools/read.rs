//! Tool `mail_read` (ADR-205): one e-mail of the person the run serves, at
//! their company's standard mail, as `mail_search` named it: its sender,
//! recipients, date and text (an HTML e-mail's as text), cut past a bound;
//! its attachments named, never read. The e-mail is not marked as read. Its
//! first megabyte alone is fetched: an attachment beyond it is not listed,
//! and the answer says so.

use async_imap::types::Fetch;
use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use validator::Validate;

use super::fetched::{fetched, seen};
use super::folders::{Examined, MESSAGE_ID_RE, MOVED, MessageId, examine};
use super::message::{attachments, body, citation, parsed, people, summary};
use super::person::{MailReach, PersonSession, failure};
use crate::outbound::{CONNECTOR_USE, Cause};

/// The most of an e-mail fetched.
pub const MAX_FETCHED_BYTES: u32 = 1024 * 1024;
/// What the server may add around it.
const ANSWER_MARGIN: usize = 64 * 1024;

pub struct MailRead {
    pub reach: MailReach,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MailReadInput {
    /// Identifiant de l’e-mail, donné par mail_search
    #[schemars(regex(pattern = r"^[is][0-9]{1,10}\.[0-9]{1,10}$"))]
    #[validate(regex(path = *MESSAGE_ID_RE, message = "Identifiant d’e-mail invalide"))]
    id: String,
}

/// The e-mail `id`, fetched from its folder, if it is still there.
async fn read(person: &mut PersonSession, id: MessageId) -> Result<(Examined, Fetch), Cause> {
    let moved = || Cause {
        summary: MOVED,
        detail: String::new(),
    };
    let examined = examine(person, id.folder).await?;
    if examined.validity != id.validity {
        return Err(moved());
    }
    person.grant(MAX_FETCHED_BYTES as usize + ANSWER_MARGIN);
    let budget = person.budget.clone();
    let query = format!("(UID FLAGS RFC822.SIZE BODY.PEEK[]<0.{MAX_FETCHED_BYTES}>)");
    let stream = person
        .session
        .uid_fetch(id.uid.to_string(), query)
        .await
        .map_err(|error| failure(&budget, error))?;
    let asked = |fetch: &Fetch| fetch.uid == Some(id.uid) && fetch.body().is_some();
    let kept = fetched(stream, &budget, asked, 1).await?;
    let fetch = kept.into_iter().next().ok_or_else(moved)?;
    Ok((examined, fetch))
}

/// What the model is told of the e-mail.
fn content(id: &str, fetch: &Fetch) -> (Value, mimorya_core::tools::Citation) {
    let message = fetch.body().and_then(parsed);
    let mut content = message.as_deref().map_or_else(|| json!({}), summary);
    if let Some(message) = &message {
        let (text, cut) = body(message);
        content["cc"] = people(message.cc());
        content["texte"] = json!(text);
        content["texteTronque"] = json!(cut);
        content["piecesJointes"] = attachments(message);
    }
    content["id"] = json!(id);
    content["lu"] = json!(seen(fetch));
    content["octets"] = json!(fetch.size);
    if fetch.size.is_some_and(|size| size > MAX_FETCHED_BYTES) {
        content["incomplet"] = json!(
            "Seul le premier mégaoctet de l’e-mail est lu : des pièces jointes au-delà ne sont \
             pas listées"
        );
    }
    (content, citation(id, message.as_deref()))
}

#[async_trait]
impl TypedTool for MailRead {
    type Input = MailReadInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "mail_read",
            "Lecture d’un e-mail (messagerie de l’entreprise)",
            "Renvoie un e-mail de la boîte de la personne servie, sur la messagerie de l’entreprise : expéditeur, destinataires, date et texte (tronqué au-delà de 20 000 caractères). Les pièces jointes sont nommées, pas lues. L’e-mail n’est pas marqué comme lu.",
            ToolRisk::Read,
        )
        .requires([CONNECTOR_USE])
    }

    async fn run(&self, input: MailReadInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let Some(id) = MessageId::parse(&input.id) else {
            return Ok(ToolResult::error("Identifiant d’e-mail invalide"));
        };
        let found = self
            .reach
            .within(async {
                let mut person = self.reach.open(ctx).await?;
                let found = read(&mut person, id).await;
                person.finish(found).await
            })
            .await;
        let (examined, fetch) = match found {
            Ok(found) => found,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let (content, citation) = content(&examined.id(id.uid), &fetch);
        // The subject stays out of the journal: the citation carries it.
        Ok(ToolResult::ok(content)
            .with_summary("E-mail lu")
            .with_citations(vec![citation]))
    }
}
