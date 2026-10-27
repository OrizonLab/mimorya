//! A REST API's operations read from its OpenAPI 3 description (ADR-114),
//! for its administrators to review before they declare any: nothing here
//! is stored nor called. Only the JSON document they give is read (no
//! address is fetched, no `$ref` followed out of it), within bounds: its
//! size, what is read of it (each part paid for by its size each time it
//! is read, `document`), the operations converted, those offered and what
//! they weigh. Each operation offered passes the checks of a declaration
//! (`checks`), under a name no other one has; what could not be offered,
//! or was changed, is said.

mod document;
mod operation;
mod parameters;
mod text;

use std::collections::HashSet;

use mimorya_core::plain_text::is_hidden;
use serde_json::Value;

use self::document::{Document, TOO_COMPLEX};
use self::operation::convert;
use self::text::{quoted, unique};
use super::checks::operation_problem;
use super::declaration::{HttpMethod, RestOperation};
use crate::outbound::MAX_URL_CHARS;

/// What a document may weigh: a large API's description fits, and so it
/// does escaped in the body of a request to the platform (1 MB).
pub const MAX_DOCUMENT_BYTES: usize = 768 * 1024;
/// Operations offered, among which the administrators keep those to declare.
pub const MAX_IMPORTED_OPERATIONS: usize = 200;
/// What the operations offered may weigh together, as JSON.
pub const MAX_OFFERED_BYTES: usize = 2 * 1024 * 1024;
/// Notes kept, the others counted.
pub const MAX_NOTES: usize = 100;
/// What may be read of the document: a unit per part, and one more per 64
/// bytes of it, each time it is read (`document`).
const MAX_READS: usize = 200_000;
/// Operations converted, offered or not: each reads its own texts.
const MAX_CONVERSIONS: usize = 1000;
const METHODS: [(&str, HttpMethod); 5] = [
    ("get", HttpMethod::Get),
    ("put", HttpMethod::Put),
    ("post", HttpMethod::Post),
    ("delete", HttpMethod::Delete),
    ("patch", HttpMethod::Patch),
];
const OTHER_METHODS: [&str; 3] = ["head", "options", "trace"];

/// The operations a description offers, in the order written.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The API's address, when the document gives an absolute one.
    pub url: Option<String>,
    pub operations: Vec<RestOperation>,
    /// What was left aside or changed, and why (in French).
    pub notes: Vec<String>,
}

/// The operations of an OpenAPI 3 description in JSON, or why it cannot be
/// read (in French).
pub fn import_openapi(document: &str) -> Result<Imported, String> {
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err(format!(
            "Description trop longue : {} Kio au plus",
            MAX_DOCUMENT_BYTES / 1024
        ));
    }
    let root: Value = serde_json::from_str(document).map_err(|error| {
        format!(
            "Description illisible (ligne {}, colonne {}) : un document JSON est attendu ; \
             convertissez d’abord une description YAML",
            error.line(),
            error.column()
        )
    })?;
    if root.get("swagger").is_some() {
        return Err("Description Swagger 2.0 : convertissez-la d’abord en OpenAPI 3".into());
    }
    if !root["openapi"]
        .as_str()
        .is_some_and(|v| v.starts_with("3."))
    {
        return Err("Description OpenAPI 3 attendue (champ « openapi » : 3.0 ou 3.1)".into());
    }
    let Some(paths) = root["paths"].as_object() else {
        return Err("La description ne contient aucun chemin (« paths »)".into());
    };
    let mut notes = Notes::default();
    let url = server_url(&root, &mut notes);
    let mut doc = Document::new(&root, MAX_READS);
    let mut operations = Vec::new();
    let mut taken = HashSet::new();
    let (mut beyond, mut unread, mut read, mut offered) = (0, 0, 0, 0_usize);
    let mut full = false;
    for (path, item) in paths {
        if doc.exhausted() || read >= MAX_CONVERSIONS {
            unread += 1;
            continue;
        }
        let item = match doc.resolve(item) {
            Ok(item) => item,
            Err(why) => {
                notes.push(format!("{} : laissé de côté, {why}", quoted(path)));
                continue;
            }
        };
        // Looked up, never listed: a path item many paths share may hold
        // any number of other keys.
        for name in OTHER_METHODS
            .iter()
            .filter(|name| item.get(**name).is_some())
        {
            let method = name.to_ascii_uppercase();
            notes.push(format!(
                "{method} {} : méthode non prise en charge",
                quoted(path)
            ));
        }
        for (name, method) in METHODS {
            let Some(operation) = item.get(name) else {
                continue;
            };
            if full || operations.len() >= MAX_IMPORTED_OPERATIONS {
                beyond += 1;
                continue;
            }
            read += 1;
            let label = format!("{} {}", method.as_str(), quoted(path));
            let mut candidate = match convert(&mut doc, path, method, item, operation) {
                Ok(candidate) => candidate,
                Err(why) => {
                    notes.push(format!("{label} : laissée de côté, {why}"));
                    continue;
                }
            };
            candidate.operation.name = unique(&candidate.operation.name, &taken);
            if let Some(problem) = operation_problem(&candidate.operation) {
                notes.push(format!("{label} : laissée de côté. {problem}"));
                continue;
            }
            let size =
                serde_json::to_vec(&candidate.operation).map_or(usize::MAX, |json| json.len());
            if offered.saturating_add(size) > MAX_OFFERED_BYTES {
                full = true;
                beyond += 1;
                continue;
            }
            offered += size;
            taken.insert(candidate.operation.name.clone());
            for note in candidate.notes {
                notes.push(format!("{label} : {note}"));
            }
            operations.push(candidate.operation);
        }
    }
    // What concerns the whole description comes first, never counted out.
    let mut said = Vec::new();
    if beyond > 0 {
        said.push(format!(
            "{beyond} opération(s) au-delà de ce qu’une réponse porte \
             ({MAX_IMPORTED_OPERATIONS} opérations, {} Mio) laissée(s) de côté : retirez du \
             document celles qui ne servent pas",
            MAX_OFFERED_BYTES / (1024 * 1024)
        ));
    }
    if unread > 0 {
        said.push(format!(
            "{unread} chemin(s) non lu(s) : {TOO_COMPLEX} (trop de parties ou d’opérations à \
             lire)"
        ));
    }
    said.extend(notes.into_vec());
    Ok(Imported {
        url,
        operations,
        notes: said,
    })
}

/// The address of the document's first server, its variables replaced by
/// their default value; `None` (and a note) when it gives no absolute one.
fn server_url(root: &Value, notes: &mut Notes) -> Option<String> {
    let server = &root["servers"][0];
    let missing = |notes: &mut Notes, why: &str| {
        notes.push(format!("Adresse de l’API à indiquer : {why}"));
        None
    };
    let Some(written) = server["url"].as_str() else {
        return missing(notes, "la description n’en donne pas");
    };
    let mut url = String::new();
    let mut rest = written;
    while let Some(start) = rest.find('{') {
        let Some(end) = rest[start..].find('}').map(|offset| start + offset) else {
            return missing(notes, "accolade ouverte sans être fermée");
        };
        let name = &rest[start + 1..end];
        let Some(value) = server["variables"][name]["default"].as_str() else {
            return missing(notes, &format!("variable « {} » sans valeur", quoted(name)));
        };
        url.push_str(&rest[..start]);
        url.push_str(value);
        rest = &rest[end + 1..];
    }
    url.push_str(rest);
    let url = url.trim().trim_end_matches('/');
    let absolute = url.starts_with("https://") || url.starts_with("http://");
    let readable = url.chars().count() <= MAX_URL_CHARS
        && !url.chars().any(|c| c.is_whitespace() || is_hidden(c));
    if !absolute || !readable {
        return missing(
            notes,
            &format!("« {} » n’est pas une adresse complète", quoted(url)),
        );
    }
    Some(url.to_string())
}

/// The notes kept, and how many more there were.
#[derive(Default)]
struct Notes {
    kept: Vec<String>,
    more: usize,
}

impl Notes {
    /// A note shows all it holds: what it quotes of the document (a
    /// character a path may not hold…) has no hidden character left.
    fn push(&mut self, note: String) {
        if self.kept.len() < MAX_NOTES {
            let shown = note
                .chars()
                .map(|c| if is_hidden(c) { '\u{FFFD}' } else { c });
            self.kept.push(shown.collect());
        } else {
            self.more += 1;
        }
    }

    fn into_vec(mut self) -> Vec<String> {
        if self.more > 0 {
            self.kept
                .push(format!("… et {} autre(s) remarque(s)", self.more));
        }
        self.kept
    }
}
