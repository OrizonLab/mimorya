//! One OpenAPI operation as a REST operation (ADR-114): named from its
//! `operationId` (or its method and path), titled by its summary, described
//! by its description, its parameters gathered by `parameters`. A deprecated
//! operation is left aside: an API keeps it only for those already using it.

use serde_json::Value;

use super::document::Document;
use super::parameters::gather;
use super::text::{bounded, one_line, operation_name};
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
    let stripped = &mut gathered.stripped;
    let id = operation["operationId"].as_str();
    let written = |key: &str| {
        operation[key]
            .as_str()
            .filter(|text| !text.trim().is_empty())
    };
    let request = format!("{} {path}", method.as_str());
    let title = written("summary")
        .or(id.filter(|id| !id.trim().is_empty()))
        .unwrap_or(&request);
    let description = written("description")
        .or_else(|| written("summary"))
        .unwrap_or(&request);
    let operation = RestOperation {
        name: operation_name(id, method, path),
        title: one_line(title, MAX_TITLE_CHARS, stripped),
        description: bounded(description, MAX_DESCRIPTION_CHARS, stripped),
        method,
        path: path.to_string(),
        parameters: gathered.parameters,
    };
    if gathered.stripped {
        gathered
            .notes
            .push("caractères invisibles retirés de ses textes".into());
    }
    Ok(Candidate {
        operation,
        notes: gathered.notes,
    })
}
