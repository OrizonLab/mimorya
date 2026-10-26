//! What a call to an operation sends (ADR-112): the input the model may
//! give (the operation's parameters, typed, nothing else), then the request
//! made of it. A path parameter is percent-encoded whole (`/`, `%`, `?` and
//! `#` included). Since a server may decode `%2F` (or `%5C`) back into a
//! separator before resolving the path, and some decode twice, a path is
//! refused when it would hold a `.` or `..` segment as sent, decoded once,
//! or decoded twice as a whole (an escape spanning two values): the request
//! never leaves the API's address nor the path its administrators wrote.
//! Query parameters are encoded as a form; body parameters make a JSON
//! object.

use serde_json::{Map, Value, json};
use url::Url;

use super::declaration::{ParameterLocation, ParameterType, RestOperation, RestParameter};

/// The longest text a parameter may bring.
pub const MAX_TEXT_CHARS: usize = 4000;
const CLIMBS: &str =
    "texte sans segment . ou .. (entre deux / ou \\), qui ferait remonter le chemin";

/// The JSON Schema the model reads: the operation's parameters, nothing else.
pub fn input_schema(operation: &RestOperation) -> Value {
    let mut properties = Map::new();
    for parameter in &operation.parameters {
        let mut schema = json!({ "type": parameter.kind.as_str() });
        if parameter.kind == ParameterType::String {
            schema["maxLength"] = json!(MAX_TEXT_CHARS);
        }
        if !parameter.description.is_empty() {
            schema["description"] = json!(parameter.description);
        }
        properties.insert(parameter.name.clone(), schema);
    }
    let required: Vec<&str> = operation
        .parameters
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.as_str())
        .collect();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

/// The input as it will be sent, or what is wrong with it (in French, for
/// the model): an object of declared parameters, each of its type.
pub fn parse_input(operation: &RestOperation, raw: &Value) -> Result<Value, Vec<String>> {
    let Some(object) = raw.as_object() else {
        return Err(vec!["(racine) : objet attendu".into()]);
    };
    let mut issues: Vec<String> = object
        .keys()
        .filter(|key| !operation.parameters.iter().any(|p| &p.name == *key))
        .map(|key| format!("{key} : paramètre inconnu de cette opération"))
        .collect();
    let mut input = Map::new();
    for parameter in &operation.parameters {
        match object.get(&parameter.name) {
            None if parameter.required => issues.push(format!("{} : requis", parameter.name)),
            None => {}
            Some(value) => match value_problem(parameter, value) {
                Some(problem) => issues.push(format!("{} : {problem}", parameter.name)),
                None => {
                    input.insert(parameter.name.clone(), value.clone());
                }
            },
        }
    }
    if issues.is_empty() {
        Ok(Value::Object(input))
    } else {
        Err(issues)
    }
}

fn value_problem(parameter: &RestParameter, value: &Value) -> Option<String> {
    let fits = match parameter.kind {
        ParameterType::String => {
            let Some(text) = value.as_str() else {
                return Some("texte attendu".into());
            };
            if text.chars().count() > MAX_TEXT_CHARS {
                return Some(format!("{MAX_TEXT_CHARS} caractères au plus"));
            }
            if parameter.location == ParameterLocation::Path {
                if text.is_empty() {
                    return Some("texte non vide attendu".into());
                }
                if climbs(&percent_decoded(text)) {
                    return Some(CLIMBS.into());
                }
            }
            true
        }
        ParameterType::Integer => value.is_i64() || value.is_u64(),
        ParameterType::Number => value.is_number(),
        ParameterType::Boolean => value.is_boolean(),
    };
    (!fits).then(|| {
        match parameter.kind {
            ParameterType::Integer => "nombre entier attendu",
            ParameterType::Number => "nombre attendu",
            _ => "booléen attendu",
        }
        .into()
    })
}

/// A request to send: its address and, for a method that has one, its body.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    pub url: Url,
    pub body: Option<Value>,
}

/// The request an input (from `parse_input`) makes of the operation, at the
/// API's address `base`; why none can be made, in French.
pub fn build(base: &str, operation: &RestOperation, input: &Value) -> Result<Built, String> {
    // As sent, and as a server decoding it once reads it: the path written
    // (which holds no `%`) and the values as they are.
    let mut path = String::new();
    let mut read = String::new();
    let mut rest = operation.path.as_str();
    while let Some(start) = rest.find('{') {
        let end = rest[start..]
            .find('}')
            .map(|offset| start + offset)
            .ok_or("chemin de l’opération illisible")?;
        let name = &rest[start + 1..end];
        let value = text(input.get(name).ok_or_else(|| format!("{name} : requis"))?);
        for (built, piece) in [(&mut path, encoded(&value)), (&mut read, value)] {
            built.push_str(&rest[..start]);
            built.push_str(&piece);
        }
        rest = &rest[end + 1..];
    }
    for built in [&mut path, &mut read] {
        built.push_str(rest);
    }
    // Decoded once more as a whole, as a server decoding twice reads it: an
    // escape may span two values, or a value and the path written after it.
    let twice = percent_decoded(&read);
    if [&path, &read, &twice].iter().any(|built| climbs(built)) {
        return Err("le chemin obtenu contient un segment . ou ..".into());
    }
    let base_url = Url::parse(base).map_err(|_| "adresse de l’API illisible")?;
    let mut url = Url::parse(&format!("{}{path}", base.trim_end_matches('/')))
        .map_err(|_| "adresse de la requête illisible")?;
    // Never somewhere else than the API's address, under its path.
    let under = format!("{}/", base_url.path().trim_end_matches('/'));
    let within = url.origin() == base_url.origin() && url.path().starts_with(&under);
    if !within {
        return Err("la requête sortirait de l’adresse de l’API".into());
    }
    let mut query = Vec::new();
    let mut body = Map::new();
    for parameter in &operation.parameters {
        let Some(value) = input.get(&parameter.name) else {
            continue;
        };
        match parameter.location {
            ParameterLocation::Path => {}
            ParameterLocation::Query => query.push((parameter.name.as_str(), text(value))),
            ParameterLocation::Body => {
                body.insert(parameter.name.clone(), value.clone());
            }
        }
    }
    url.set_query(None);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    let has_body = operation
        .parameters
        .iter()
        .any(|p| p.location == ParameterLocation::Body);
    Ok(Built {
        url,
        body: has_body.then_some(Value::Object(body)),
    })
}

/// Whether a path holds a `.` or `..` segment, `\` taken for `/` as some
/// servers do.
fn climbs(path: &str) -> bool {
    path.split(['/', '\\'])
        .any(|segment| segment == "." || segment == "..")
}

/// The text with its `%XX` escapes decoded once (lossy), as a server
/// decoding a path a second time reads it. Decoding never takes a `.` or
/// `..` segment away: an escape holds neither `.`, `/` nor `\`.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%')
            .then(|| text.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                i += 3;
            }
            None => {
                decoded.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// A parameter's value as text: a string as it is, anything else as JSON
/// writes it.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Every byte but the unreserved characters of RFC 3986, percent-encoded.
fn encoded(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// What the person asked to validate reads: the request as it will be sent.
pub fn described(built: &Built, method: &str) -> String {
    match &built.body {
        Some(body) => format!("{method} {} avec le corps {body}", built.url),
        None => format!("{method} {}", built.url),
    }
}
