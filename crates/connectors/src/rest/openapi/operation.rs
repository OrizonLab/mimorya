//! One OpenAPI operation as a REST operation (ADR-114): named from its
//! `operationId` (or its method and path), titled by its summary, described
//! by its description, its parameters gathered by `parameters`. A text
//! counts once its hidden characters are out: one that showed nothing gives
//! way to the next. A deprecated operation is left aside: an API keeps it
//! only for those already using it.

use serde_json::Value;

use super::document::Document;
use super::parameters::gather;
use super::text::{beginning, bounded, one_line, operation_name};
use crate::rest::checks::{MAX_DESCRIPTION_CHARS, MAX_TITLE_CHARS};
use crate::rest::declaration::{HttpMethod, RestOperation};

/// An operation to offer, and what was left aside of it.
pub(super) struct Candidate {
    pub operation: RestOperation,
    pub notes: Vec<String>,
}

/// The operation `method` of the path item `item` at `path`, or why none.
pub(super) fn convert<'a>(
    doc: &mut Document<'a>,
    path: &str,
    method: HttpMethod,
    item: &'a Value,
    operation: &'a Value,
) -> Result<Candidate, String> {
    if !operation.is_object() {
        return Err("une opération illisible".into());
    }
    if operation["deprecated"] == true {
        return Err("une opération dépréciée".into());
    }
    let mut gathered = gather(doc, item, operation, method)?;
    let mut stripped = gathered.stripped;
    let id = operation["operationId"].as_str();
    let mut written = |key: &str, max: usize, lines: bool| -> Result<Option<String>, String> {
        let Some(text) = operation[key].as_str() else {
            return Ok(None);
        };
        let read = doc.read(text, max)?;
        let kept = if lines {
            bounded(read, max, &mut stripped)
        } else {
            one_line(read, max, &mut stripped)
        };
        Ok((!kept.is_empty() && kept != "…").then_some(kept))
    };
    let title = match written("summary", MAX_TITLE_CHARS, false)? {
        Some(title) => Some(title),
        None => written("operationId", MAX_TITLE_CHARS, false)?,
    };
    let description = match written("description", MAX_DESCRIPTION_CHARS, true)? {
        Some(description) => Some(description),
        None => written("summary", MAX_DESCRIPTION_CHARS, true)?,
    };
    // Written here, not read in the document: no hidden character.
    let request = format!("{} {path}", method.as_str());
    let named = |max| one_line(beginning(&request, max), max, &mut false);
    let operation = RestOperation {
        name: operation_name(id, method, path),
        title: title.unwrap_or_else(|| named(MAX_TITLE_CHARS)),
        description: description.unwrap_or_else(|| named(MAX_DESCRIPTION_CHARS)),
        method,
        path: path.to_string(),
        parameters: gathered.parameters,
    };
    if stripped {
        gathered
            .notes
            .push("caractères invisibles retirés de ses textes".into());
    }
    Ok(Candidate {
        operation,
        notes: gathered.notes,
    })
}
