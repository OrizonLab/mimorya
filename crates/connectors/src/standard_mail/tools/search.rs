//! Tool `mail_search` (ADR-205): the e-mails of the person the run serves,
//! at their company's standard mail, in their inbox or among those they
//! sent: found by words and since a day, or the latest; the most recent
//! first. A summary per e-mail; `mail_read` gives its text.

use std::cmp::Reverse;
use std::collections::HashSet;

use async_imap::types::Fetch;
use async_trait::async_trait;
use chrono::NaiveDate;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use validator::Validate;

use super::criteria::Criteria;
use super::fetched::{fetched, seen};
use super::folders::{Examined, Folder, examine};
use super::message::{SUMMARY_FIELDS, citation, headers, summary};
use super::person::{MailReach, PersonSession, failure};
use crate::outbound::{CONNECTOR_USE, Cause};

const DEFAULT_LIMIT: u8 = 10;
/// The most a search's answer may hold: its UIDs.
const SEARCH_BYTES: usize = 1024 * 1024;
/// The most of an e-mail's headers fetched for its summary.
const SUMMARY_BYTES: usize = 16 * 1024;
/// What the server may add around each summary (its UID, flags, size).
const SUMMARY_MARGIN: usize = 1024;

pub struct MailSearch {
    pub reach: MailReach,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MailSearchInput {
    /// Mots cherchés dans l’e-mail (objet, texte, adresses), tous présents ;
    /// sans eux, les derniers e-mails
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(length(min = 1, max = 200))]
    query: Option<String>,
    /// Reçus depuis ce jour (AAAA-MM-JJ)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    since: Option<NaiveDate>,
    /// Dossier : « inbox » (réception, par défaut) ou « sent » (envoyés)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<Folder>,
    /// Nombre d’e-mails au plus (10 par défaut)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(range(min = 1, max = 25))]
    limit: Option<u8>,
}

/// What a search found: the e-mails kept, and how many matched.
struct Found {
    examined: Examined,
    matched: usize,
    emails: Vec<Value>,
    citations: Vec<mimorya_core::tools::Citation>,
}

/// The e-mails of `folder` matching `criteria`, the `limit` most recent.
async fn search(
    person: &mut PersonSession,
    folder: Folder,
    criteria: &Criteria,
    limit: usize,
) -> Result<Found, Cause> {
    let examined = examine(person, folder).await?;
    let query = format!(
        "(UID FLAGS RFC822.SIZE BODY.PEEK[HEADER.FIELDS ({SUMMARY_FIELDS})]<0.{SUMMARY_BYTES}>)"
    );
    let per_summary = SUMMARY_BYTES + SUMMARY_MARGIN;
    let budget = person.budget.clone();
    let (fetches, matched) = if criteria.is_empty() {
        // The latest: the last in the folder, by their place.
        if examined.exists == 0 {
            (Vec::new(), 0)
        } else {
            let first = examined.exists.saturating_sub(limit as u32 - 1).max(1);
            person.grant(per_summary * limit);
            let range = first..=examined.exists;
            let stream = person
                .session
                .fetch(format!("{first}:{}", examined.exists), &query)
                .await
                .map_err(|error| failure(&budget, error))?;
            let summarised = |f: &Fetch| range.contains(&f.message) && f.header().is_some();
            let kept = fetched(stream, &budget, summarised, limit).await?;
            (kept, examined.exists as usize)
        }
    } else {
        let literal_plus = criteria.in_utf8() && takes_literals(person).await?;
        person.grant(SEARCH_BYTES);
        let found = person
            .session
            .uid_search(criteria.command(literal_plus))
            .await
            .map_err(|error| person.failed(error))?;
        let mut uids: Vec<u32> = found.into_iter().collect();
        uids.sort_unstable_by_key(|uid| Reverse(*uid));
        let matched = uids.len();
        uids.truncate(limit);
        if uids.is_empty() {
            (Vec::new(), 0)
        } else {
            person.grant(per_summary * uids.len());
            let set: Vec<String> = uids.iter().map(u32::to_string).collect();
            let wanted: HashSet<u32> = uids.into_iter().collect();
            let stream = person
                .session
                .uid_fetch(set.join(","), &query)
                .await
                .map_err(|error| failure(&budget, error))?;
            let in_wanted =
                |f: &Fetch| f.uid.is_some_and(|u| wanted.contains(&u)) && f.header().is_some();
            (fetched(stream, &budget, in_wanted, limit).await?, matched)
        }
    };
    let mut fetches = fetches;
    fetches.sort_unstable_by_key(|fetch| Reverse(fetch.uid));
    let mut emails = Vec::new();
    let mut citations = Vec::new();
    for fetch in &fetches {
        let id = examined.id(fetch.uid.unwrap_or_default());
        let message = fetch.header().and_then(headers);
        let mut email = message.as_ref().map_or_else(|| json!({}), summary);
        email["id"] = json!(id);
        email["lu"] = json!(seen(fetch));
        email["octets"] = json!(fetch.size);
        citations.push(citation(&id, message.as_ref()));
        emails.push(email);
    }
    Ok(Found {
        examined,
        matched,
        emails,
        citations,
    })
}

/// Whether the server takes literals without a pause (RFC 7888).
async fn takes_literals(person: &mut PersonSession) -> Result<bool, Cause> {
    let offered = person
        .session
        .capabilities()
        .await
        .map_err(|error| person.failed(error))?;
    Ok(offered.has_str("LITERAL+") || offered.has_str("LITERAL-"))
}

#[async_trait]
impl TypedTool for MailSearch {
    type Input = MailSearchInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "mail_search",
            "Recherche d’e-mails (messagerie de l’entreprise)",
            "Cherche dans la boîte e-mail de la personne servie, sur la messagerie de l’entreprise, avec son propre compte : par mots (tous présents dans l’objet, le texte ou les adresses) et depuis un jour si indiqués, sinon les derniers e-mails ; dans la boîte de réception ou parmi les e-mails envoyés. Les plus récents d’abord. Renvoie un résumé par e-mail ; utiliser mail_read pour en lire le texte.",
            ToolRisk::Read,
        )
        .requires([CONNECTOR_USE])
    }

    async fn run(&self, input: MailSearchInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let criteria = Criteria::new(input.query.as_deref(), input.since);
        if input.query.is_some() && criteria.is_empty() && input.since.is_none() {
            return Ok(ToolResult::error("Aucun mot à chercher"));
        }
        let limit = usize::from(input.limit.unwrap_or(DEFAULT_LIMIT));
        let folder = input.folder.unwrap_or_default();
        let found = self
            .reach
            .within(async {
                let mut person = self.reach.open(ctx).await?;
                let found = search(&mut person, folder, &criteria, limit).await;
                person.finish(found).await
            })
            .await;
        let found = match found {
            Ok(found) => found,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let count = found.emails.len();
        let mut content = json!({
            "dossier": found.examined.folder,
            "nombre": count,
            "emails": found.emails,
        });
        if found.matched > count {
            content["autres"] = json!(format!(
                "{} autre(s) e-mail(s) correspondent : précisez la recherche",
                found.matched - count
            ));
        }
        Ok(ToolResult::ok(content)
            .with_summary(format!("{count} e-mail(s) trouvé(s)"))
            .with_citations(found.citations))
    }
}
