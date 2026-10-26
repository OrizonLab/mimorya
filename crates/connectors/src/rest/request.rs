//! What a call to an operation sends (ADR-112): the input the model may
//! give (the operation's parameters, typed, nothing else), then the request
//! made of it. A path parameter is percent-encoded whole (`/`, `%`, `?` and
//! `#` included), and a path left with a `.` or `..` segment is refused: the
//! request never leaves the API's address nor the path its administrators
//! wrote. Query parameters are encoded as a form; body parameters make a
//! JSON object.

use serde_json::{Map, Value, json};
use url::Url;

use super::declaration::{ParameterLocation, ParameterType, RestOperation, RestParameter};

/// The longest text a parameter may bring.
pub const MAX_TEXT_CHARS: usize = 4000;

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
            let in_path = parameter.location == ParameterLocation::Path;
            if in_path && matches!(text, "" | "." | "..") {
                return Some("texte non vide, autre que . ou .., attendu".into());
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
    let mut path = String::new();
    let mut rest = operation.path.as_str();
    while let Some(start) = rest.find('{') {
        let end = rest[start..]
            .find('}')
            .map(|offset| start + offset)
            .ok_or("chemin de l’opération illisible")?;
        let name = &rest[start + 1..end];
        let value = input.get(name).ok_or_else(|| format!("{name} : requis"))?;
        path.push_str(&rest[..start]);
        path.push_str(&encoded(&text(value)));
        rest = &rest[end + 1..];
    }
    path.push_str(rest);
    if path
        .split('/')
        .any(|segment| segment == "." || segment == "..")
    {
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
