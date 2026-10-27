//! Tool `m365_mail_search` (ADR-117): the e-mails of the person the run
//! serves, found by words (Microsoft's search, by relevance) or the latest
//! received, since a day if given. A summary per e-mail; `m365_mail_read`
//! gives its text.

use async_trait::async_trait;
use chrono::NaiveDate;
use mimorya_core::account_links::MAIL_READ;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use validator::Validate;

use super::graph::Graph;
use super::reach::read_as_person;
use super::shape::{collection, day_start, graph_instant, person, short};
use crate::outbound::CONNECTOR_USE;

const SELECT: &str = "id,subject,from,receivedDateTime,bodyPreview,isRead,hasAttachments,webLink";
const DEFAULT_LIMIT: u8 = 10;

pub struct MailSearch {
    pub graph: Graph,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MailSearchInput {
    /// Mots cherchés (objet, texte, expéditeur) ; sans eux, les derniers e-mails reçus
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(length(min = 1, max = 200))]
    query: Option<String>,
    /// Reçus depuis ce jour (AAAA-MM-JJ) ; seulement sans mots cherchés
    #[serde(default, skip_serializing_if = "Option::is_none")]
    since: Option<NaiveDate>,
    /// Nombre d’e-mails au plus (10 par défaut)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(range(min = 1, max = 25))]
    limit: Option<u8>,
}

/// The words as Microsoft's search reads them: within quotes, so without
/// quotes nor escapes of their own.
fn search_words(query: &str) -> Option<String> {
    let words: String = query
        .chars()
        .map(|c| {
            if c == '"' || c == '\\' || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    (!words.is_empty()).then(|| format!("\"{words}\""))
}

/// The query of the search: Microsoft does not order nor filter what it
/// finds by words.
fn query(input: &MailSearchInput) -> Result<Vec<(&'static str, String)>, &'static str> {
    let top = input.limit.unwrap_or(DEFAULT_LIMIT).to_string();
    let mut query = vec![("$select", SELECT.to_string()), ("$top", top)];
    match (&input.query, input.since) {
        (Some(_), Some(_)) => {
            return Err(
                "Cherchez par mots ou par jour de réception, pas les deux : \
                 Microsoft 365 ne les combine pas",
            );
        }
        (Some(words), None) => {
            let words = search_words(words).ok_or("Aucun mot à chercher")?;
            query.push(("$search", words));
        }
        (None, since) => {
            query.push(("$orderby", "receivedDateTime desc".into()));
            if let Some(day) = since {
                let start = graph_instant(day_start(day));
                query.push(("$filter", format!("receivedDateTime ge {start}")));
            }
        }
    }
    Ok(query)
}

fn summary(message: &Value) -> Value {
    json!({
        "id": message.get("id"),
        "objet": short(message, "/subject"),
        "de": message.get("from").map_or(Value::Null, person),
        "recu": short(message, "/receivedDateTime"),
        "apercu": short(message, "/bodyPreview"),
        "lu": message.get("isRead"),
        "piecesJointes": message.get("hasAttachments"),
        "lien": short(message, "/webLink"),
    })
}

pub fn citation(message: &Value) -> Option<Citation> {
    let id = message.get("id").and_then(Value::as_str)?;
    let subject = message
        .get("subject")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("(sans objet)");
    Some(Citation::new("email", id).labelled(super::shape::bounded(subject, 120).0))
}

#[async_trait]
impl TypedTool for MailSearch {
    type Input = MailSearchInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "m365_mail_search",
            "Recherche d’e-mails Microsoft 365",
            "Cherche dans la boîte e-mail Microsoft 365 de la personne servie, avec ses propres droits : par mots (les plus pertinents), ou les derniers reçus, depuis un jour si indiqué. Renvoie un résumé par e-mail ; utiliser m365_mail_read pour en lire le texte.",
            ToolRisk::Read,
        )
        .requires([CONNECTOR_USE])
    }

    async fn run(&self, input: MailSearchInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let query = match query(&input) {
            Ok(query) => query,
            Err(problem) => return Ok(ToolResult::error(problem)),
        };
        let url = self.graph.address(&["me", "messages"], &query);
        let answer = match read_as_person(&self.graph, ctx, &[MAIL_READ], url, None).await {
            Ok(answer) => answer,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let (messages, more) = collection(&answer);
        let mut content = json!({
            "nombre": messages.len(),
            "emails": messages.iter().map(summary).collect::<Vec<_>>(),
        });
        if more {
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

    fn input(value: Value) -> MailSearchInput {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn words_are_searched_without_their_quotes() {
        assert_eq!(
            search_words("  facture \"Dupont\"\\ mars "),
            Some("\"facture Dupont mars\"".into())
        );
        assert_eq!(search_words("\" \\ "), None);
    }

    #[test]
    fn words_and_a_day_are_not_combined() {
        let both = input(json!({ "query": "facture", "since": "2026-10-01" }));
        assert!(query(&both).is_err());
        let latest = input(json!({ "since": "2026-10-01" }));
        let latest = query(&latest).unwrap();
        assert!(latest.contains(&("$filter", "receivedDateTime ge 2026-09-30T22:00:00Z".into())));
        assert!(latest.contains(&("$orderby", "receivedDateTime desc".into())));
        assert!(latest.contains(&("$top", "10".into())));
    }
}
