//! Tool `google_gmail_search` (ADR-122): the e-mails of the person the run
//! serves, found by Gmail's search (its words and operators, `from:`,
//! `subject:`…) or the latest received, since a day if given, the latest
//! first. Gmail lists identifiers only: each e-mail's headers are then read
//! with the same token, a few at a time. A summary per e-mail;
//! `google_gmail_read` gives its text.

use async_trait::async_trait;
use chrono::NaiveDate;
use futures::{StreamExt, TryStreamExt, stream};
use mimorya_core::account_links::GMAIL_READONLY;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;
use validator::Validate;

use super::message::{citation, header_of, link, received};
use super::text::decode_entities;
use crate::delegated::shape::{MAX_SHORT_CHARS, bounded, day_start};
use crate::delegated::{AsPerson, ProviderApi};
use crate::outbound::CONNECTOR_USE;

const DEFAULT_LIMIT: u8 = 10;
/// The e-mails whose headers are read at once.
const READ_AT_ONCE: usize = 5;

pub struct GmailSearch {
    pub gmail: ProviderApi,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GmailSearchInput {
    /// Recherche Gmail (mots, ou opérateurs comme from:, subject:, has:attachment) ; sans elle, les derniers e-mails reçus
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(length(min = 1, max = 200))]
    query: Option<String>,
    /// Reçus depuis ce jour (AAAA-MM-JJ)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    since: Option<NaiveDate>,
    /// Nombre d’e-mails au plus (10 par défaut)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(range(min = 1, max = 25))]
    limit: Option<u8>,
}

/// Gmail's query: the words, on one line, and the day as the instant it
/// starts in the platform's zone (Gmail reads a bare date in its own).
fn search(input: &GmailSearchInput) -> Result<Option<String>, &'static str> {
    let mut terms = Vec::new();
    if let Some(words) = &input.query {
        let words: String = words
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
        if words.is_empty() {
            return Err("Aucun mot à chercher");
        }
        terms.push(words);
    }
    if let Some(day) = input.since {
        terms.push(format!("after:{}", day_start(day).timestamp()));
    }
    Ok((!terms.is_empty()).then(|| terms.join(" ")))
}

fn summary(message: &Value) -> Value {
    let id = message
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let unread = message
        .get("labelIds")
        .and_then(Value::as_array)
        .is_some_and(|labels| labels.iter().any(|l| l == "UNREAD"));
    let snippet = message
        .get("snippet")
        .and_then(Value::as_str)
        .map_or(Value::Null, |text| {
            json!(bounded(&decode_entities(text), MAX_SHORT_CHARS).0)
        });
    json!({
        "id": id,
        "objet": header_of(message, "Subject", MAX_SHORT_CHARS),
        "de": header_of(message, "From", MAX_SHORT_CHARS),
        "recu": received(message),
        "apercu": snippet,
        "lu": !unread,
        "lien": link(id),
    })
}

#[async_trait]
impl TypedTool for GmailSearch {
    type Input = GmailSearchInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "google_gmail_search",
            "Recherche d’e-mails Gmail",
            "Cherche dans la boîte Gmail de la personne servie (hors spam et corbeille), avec ses propres droits : par la recherche de Gmail (mots, from:, subject:, has:attachment…), ou les derniers reçus, depuis un jour si indiqué, les plus récents d’abord. Renvoie un résumé par e-mail ; utiliser google_gmail_read pour en lire le texte.",
            ToolRisk::Read,
        )
        .requires([CONNECTOR_USE])
    }

    async fn run(&self, input: GmailSearchInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let search = match search(&input) {
            Ok(search) => search,
            Err(problem) => return Ok(ToolResult::error(problem)),
        };
        let limit = input.limit.unwrap_or(DEFAULT_LIMIT).to_string();
        let mut query = vec![("maxResults", limit)];
        if let Some(search) = search {
            query.push(("q", search));
        }
        let person = match AsPerson::open(&self.gmail, ctx, &[GMAIL_READONLY]).await {
            Ok(person) => person,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let url = self.gmail.address(&["users", "me", "messages"], &query);
        let listed = match person.get(url, None).await {
            Ok(listed) => listed,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        // Each e-mail's headers, by the addresses of the identifiers listed.
        let addresses: Vec<Url> = listed
            .get("messages")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter_map(|m| m.get("id").and_then(Value::as_str))
            .map(|id| {
                self.gmail.address(
                    &["users", "me", "messages", id],
                    &[
                        ("format", "metadata".into()),
                        ("metadataHeaders", "Subject".into()),
                        ("metadataHeaders", "From".into()),
                    ],
                )
            })
            .collect();
        // The first refusal stops the reads; an e-mail deleted since it was
        // listed is passed over.
        let read: Result<Vec<Option<Value>>, String> = stream::iter(addresses)
            .map(|url| person.find(url, None))
            .buffered(READ_AT_ONCE)
            .try_collect()
            .await;
        let messages: Vec<Value> = match read {
            Ok(read) => read.into_iter().flatten().collect(),
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let mut content = json!({
            "nombre": messages.len(),
            "emails": messages.iter().map(summary).collect::<Vec<_>>(),
        });
        if listed.get("nextPageToken").is_some() {
            content["autres"] = json!("D’autres e-mails correspondent : précisez la recherche");
        }
        Ok(ToolResult::ok(content)
            .with_summary(format!("{} e-mail(s) trouvé(s)", messages.len()))
            .with_citations(messages.iter().filter_map(citation).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(value: Value) -> GmailSearchInput {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn words_and_a_day_make_one_search() {
        let both = input(json!({ "query": " facture\n from:fournisseur ", "since": "2026-10-01" }));
        assert_eq!(
            search(&both),
            Ok(Some("facture from:fournisseur after:1790805600".into()))
        );
        assert_eq!(search(&input(json!({}))), Ok(None));
        assert!(search(&input(json!({ "query": " \t " }))).is_err());
    }

    #[test]
    fn a_summary_reads_headers_and_labels() {
        let message = json!({
            "id": "18c1",
            "snippet": "L&#39;offre &amp; le devis",
            "internalDate": "1761562800000",
            "labelIds": ["INBOX", "UNREAD"],
            "payload": { "headers": [
                { "name": "Subject", "value": "Devis" },
                { "name": "From", "value": "Fournisseur <contact@fournisseur.example>" },
            ] },
        });
        let read = summary(&message);
        assert_eq!(read["objet"], "Devis");
        assert_eq!(read["apercu"], "L'offre & le devis");
        assert_eq!(read["recu"], "2025-10-27T11:00:00Z");
        assert_eq!(read["lu"], false);
        assert_eq!(read["lien"], "https://mail.google.com/mail/#all/18c1");
        assert_eq!(citation(&message).unwrap().id, "18c1");
    }
}
