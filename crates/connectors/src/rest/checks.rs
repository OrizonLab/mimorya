//! What a REST API's declaration may hold (ADR-112), checked when an
//! administrator saves it and again at each start (the operator may have
//! narrowed the network policy): the prefix and address rules every company
//! connector follows (`outbound`), a token header that is no part of the
//! protocol, and operations whose names, texts, paths and parameters are
//! bounded, visible and consistent with each other. A path never holds `%`,
//! `?`, `#` nor a `.` or `..` segment: what a parameter brings is encoded
//! (`request`), so that a request never leaves the path written here.

use std::collections::HashSet;

use mimorya_core::plain_text::shows_all_it_holds;

use super::declaration::{ParameterLocation, RestConfig, RestOperation, RestParameter};
use crate::outbound::{NetworkPolicy, prefix_problem, url_problem};

pub const MAX_OPERATIONS: usize = 50;
pub const MAX_PARAMETERS: usize = 30;
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_DESCRIPTION_CHARS: usize = 1000;
pub const MAX_PARAMETER_DESCRIPTION_CHARS: usize = 300;
pub const MAX_PATH_CHARS: usize = 500;
/// What one API's declarations may add to every model call: as much as an
/// MCP server's (ADR-025).
pub const MAX_CONFIG_BYTES: usize = 256 * 1024;
const MAX_OPERATION_NAME: usize = 40;
const MAX_PARAMETER_NAME: usize = 64;
const MAX_HEADER_NAME: usize = 64;
/// Characters a path may hold besides letters, digits and placeholders: the
/// unreserved ones, sub-delimiters, `:`, `@` and `/` (RFC 3986).
const PATH_PUNCTUATION: &str = "-._~!$&'()*+,;=:@/";
/// Headers the protocol, the client or a proxy own.
const RESERVED_HEADERS: [&str; 14] = [
    "authorization",
    "host",
    "content-length",
    "content-type",
    "content-encoding",
    "transfer-encoding",
    "connection",
    "keep-alive",
    "upgrade",
    "te",
    "trailer",
    "expect",
    "cookie",
    "accept",
];

/// Why this declaration cannot be used (in French), if it cannot.
pub fn config_problem(config: &RestConfig, network: NetworkPolicy) -> Option<String> {
    prefix_problem(&config.prefix)
        .or_else(|| url_problem(&config.url, network))
        .or_else(|| config.token_header.as_deref().and_then(header_problem))
        .or_else(|| operations_problem(&config.operations))
        .or_else(|| size_problem(config))
}

/// Why a token may not be sent in this header, if it may not.
pub fn header_problem(header: &str) -> Option<String> {
    let well_formed = header
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && header.len() <= MAX_HEADER_NAME
        && header
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !well_formed {
        return Some(format!(
            "En-tête du jeton invalide : une lettre puis des lettres sans accent, des chiffres \
             ou - ({MAX_HEADER_NAME} au plus)"
        ));
    }
    let lower = header.to_ascii_lowercase();
    let reserved = RESERVED_HEADERS.contains(&lower.as_str())
        || lower.starts_with("proxy-")
        || lower.starts_with("sec-");
    reserved.then(|| {
        format!(
            "En-tête réservé : « {header} ». Choisissez celui où l’API attend sa clé (par \
             exemple X-API-Key), ou aucun pour « Authorization: Bearer »"
        )
    })
}

fn operations_problem(operations: &[RestOperation]) -> Option<String> {
    if operations.is_empty() {
        return Some("Une opération au moins".into());
    }
    if operations.len() > MAX_OPERATIONS {
        return Some(format!("{MAX_OPERATIONS} opérations au plus"));
    }
    let mut names = HashSet::new();
    for operation in operations {
        if !names.insert(operation.name.as_str()) {
            return Some(format!(
                "Opération « {} » déclarée deux fois",
                operation.name
            ));
        }
        if let Some(problem) = operation_problem(operation) {
            return Some(problem);
        }
    }
    None
}

fn operation_problem(operation: &RestOperation) -> Option<String> {
    let name = &operation.name;
    let mut chars = name.chars();
    let well_formed = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && (2..=MAX_OPERATION_NAME).contains(&name.len());
    if !well_formed {
        return Some(format!(
            "Nom d’opération invalide « {name} » : une minuscule puis des minuscules, des \
             chiffres ou _ (2 à {MAX_OPERATION_NAME})"
        ));
    }
    if !one_line(&operation.title, 1, MAX_TITLE_CHARS) {
        return Some(format!(
            "Opération {name} : intitulé de 1 à {MAX_TITLE_CHARS} caractères, sur une ligne, \
             du texte visible"
        ));
    }
    let description = &operation.description;
    let length = description.chars().count();
    if description.trim().is_empty()
        || length > MAX_DESCRIPTION_CHARS
        || !shows_all_it_holds(description)
    {
        return Some(format!(
            "Opération {name} : description de 1 à {MAX_DESCRIPTION_CHARS} caractères, du \
             texte visible, des retours à la ligne et des tabulations seulement"
        ));
    }
    let placeholders = match path_placeholders(&operation.path) {
        Ok(placeholders) => placeholders,
        Err(problem) => return Some(format!("Opération {name} : {problem}")),
    };
    parameters_problem(operation, &placeholders)
        .map(|problem| format!("Opération {name} : {problem}"))
}

fn parameters_problem(operation: &RestOperation, placeholders: &[String]) -> Option<String> {
    if operation.parameters.len() > MAX_PARAMETERS {
        return Some(format!("{MAX_PARAMETERS} paramètres au plus"));
    }
    let mut names = HashSet::new();
    for parameter in &operation.parameters {
        if !names.insert(parameter.name.as_str()) {
            return Some(format!(
                "paramètre « {} » déclaré deux fois",
                parameter.name
            ));
        }
        if let Some(problem) = parameter_problem(operation, parameter, placeholders) {
            return Some(problem);
        }
    }
    placeholders
        .iter()
        .find(|placeholder| {
            !operation
                .parameters
                .iter()
                .any(|p| p.location == ParameterLocation::Path && &p.name == *placeholder)
        })
        .map(|placeholder| {
            format!("{{{placeholder}}} figure dans le chemin sans être un paramètre de chemin")
        })
}

fn parameter_problem(
    operation: &RestOperation,
    parameter: &RestParameter,
    placeholders: &[String],
) -> Option<String> {
    let name = &parameter.name;
    if !parameter_name(name) {
        return Some(format!(
            "nom de paramètre invalide « {name} » : une lettre puis des lettres sans accent, des \
             chiffres, _ - ou . ({MAX_PARAMETER_NAME} au plus)"
        ));
    }
    if !parameter.description.is_empty()
        && !one_line(&parameter.description, 1, MAX_PARAMETER_DESCRIPTION_CHARS)
    {
        return Some(format!(
            "description du paramètre « {name} » : {MAX_PARAMETER_DESCRIPTION_CHARS} \
             caractères au plus, sur une ligne, du texte visible"
        ));
    }
    match parameter.location {
        ParameterLocation::Path if !parameter.required => Some(format!(
            "le paramètre de chemin « {name} » est toujours requis"
        )),
        ParameterLocation::Path if !placeholders.contains(name) => {
            Some(format!("le chemin ne contient pas {{{name}}}"))
        }
        ParameterLocation::Body if !operation.method.has_body() => Some(format!(
            "une requête {} n’a pas de corps : le paramètre « {name} » va dans le chemin ou \
             les paramètres de la requête",
            operation.method.as_str()
        )),
        _ => None,
    }
}

/// The placeholders of a path (`{name}`), or why it is no path to follow
/// the API's address with.
pub fn path_placeholders(path: &str) -> Result<Vec<String>, String> {
    if !path.starts_with('/') || path.chars().count() > MAX_PATH_CHARS {
        return Err(format!(
            "chemin de 1 à {MAX_PATH_CHARS} caractères commençant par /"
        ));
    }
    let mut placeholders = Vec::new();
    let mut rest = path;
    while let Some(c) = rest.chars().next() {
        if c == '{' {
            let Some(end) = rest.find('}') else {
                return Err("accolade ouverte sans être fermée dans le chemin".into());
            };
            let name = &rest[1..end];
            if !parameter_name(name) {
                return Err(format!(
                    "{{{name}}} : nom de paramètre invalide dans le chemin"
                ));
            }
            placeholders.push(name.to_string());
            rest = &rest[end + 1..];
            continue;
        }
        if !(c.is_ascii_alphanumeric() || PATH_PUNCTUATION.contains(c)) {
            return Err(format!(
                "caractère « {c} » refusé dans le chemin : lettres sans accent, chiffres, \
                 {PATH_PUNCTUATION} et {{paramètre}} seulement"
            ));
        }
        rest = &rest[c.len_utf8()..];
    }
    if path
        .split('/')
        .any(|segment| segment == "." || segment == "..")
    {
        return Err("segment . ou .. refusé dans le chemin".into());
    }
    Ok(placeholders)
}

/// A letter then letters, digits, `_`, `-` or `.`.
fn parameter_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name.len() <= MAX_PARAMETER_NAME
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c))
}

/// Visible text on one line, of `min` to `max` characters.
fn one_line(text: &str, min: usize, max: usize) -> bool {
    let length = text.chars().count();
    (min..=max).contains(&length)
        && !text.trim().is_empty()
        && !text.contains(['\n', '\t'])
        && shows_all_it_holds(text)
}

fn size_problem(config: &RestConfig) -> Option<String> {
    let bytes = serde_json::to_vec(config).map_or(usize::MAX, |json| json.len());
    (bytes > MAX_CONFIG_BYTES).then(|| {
        format!(
            "Déclaration trop longue : {} Kio au plus pour l’ensemble des opérations",
            MAX_CONFIG_BYTES / 1024
        )
    })
}
