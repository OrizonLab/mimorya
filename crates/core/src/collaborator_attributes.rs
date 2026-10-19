//! Rules for the attributes of a collaborator (`users.attributes`), which
//! access policy conditions read as `subject.attributes.<key>` (ADR-023). An
//! administrator sets them; values stay short and typed, so that a condition
//! compares them reliably and nothing sensitive lands there by accident.

use crate::plain_text::{canonical, is_hidden};
use serde_json::{Map, Number, Value};

pub const MAX_ATTRIBUTES: usize = 20;
pub const MAX_ATTRIBUTE_KEY_CHARS: usize = 40;
pub const MAX_ATTRIBUTE_TEXT_CHARS: usize = 200;
pub const MAX_ATTRIBUTE_LIST_ITEMS: usize = 20;
/// Integers beyond this lose precision in the browser (`Number.MAX_SAFE_INTEGER`).
pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// The attributes with their texts trimmed, or why they are refused (in
/// French). A key starts with a lowercase letter followed by letters and
/// digits (`department`, `contractType`), since a condition reads it in a
/// dotted path; a value is a text, a number, yes/no, or a list of texts.
pub fn normalize_attributes(attributes: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    if attributes.len() > MAX_ATTRIBUTES {
        return Err(format!(
            "Un collaborateur porte au plus {MAX_ATTRIBUTES} attributs"
        ));
    }
    attributes
        .iter()
        .map(|(key, value)| {
            key_problem(key).map_or_else(|| Ok((key.clone(), normalized(key, value)?)), Err)
        })
        .collect()
}

fn key_problem(key: &str) -> Option<String> {
    let mut chars = key.chars();
    let well_formed = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_alphanumeric())
        && key.len() <= MAX_ATTRIBUTE_KEY_CHARS;
    (!well_formed).then(|| {
        format!(
            "Nom d’attribut invalide : « {key} » (une minuscule puis des lettres ou des chiffres, \
             {MAX_ATTRIBUTE_KEY_CHARS} caractères au plus)"
        )
    })
}

fn normalized(key: &str, value: &Value) -> Result<Value, String> {
    match value {
        Value::String(text) => Ok(Value::String(text_value(key, text)?)),
        Value::Number(number) if !is_exact(number) => Err(format!(
            "« {key} » : entier trop grand (±{MAX_SAFE_INTEGER} au plus)"
        )),
        Value::Number(_) | Value::Bool(_) => Ok(value.clone()),
        Value::Array(items) if !items.is_empty() && items.len() <= MAX_ATTRIBUTE_LIST_ITEMS => {
            items
                .iter()
                .map(|item| match item {
                    Value::String(text) => Ok(Value::String(text_value(key, text)?)),
                    _ => Err(format!("« {key} » : une liste ne contient que des textes")),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        Value::Array(_) => Err(format!(
            "« {key} » : une liste compte de 1 à {MAX_ATTRIBUTE_LIST_ITEMS} textes"
        )),
        Value::Null | Value::Object(_) => Err(format!(
            "« {key} » : la valeur est un texte, un nombre, oui/non ou une liste de textes"
        )),
    }
}

/// Whether the number reads back the same everywhere (decimals always do).
fn is_exact(number: &Number) -> bool {
    match (number.as_i64(), number.as_u64()) {
        (_, Some(n)) => n <= MAX_SAFE_INTEGER,
        (Some(n), None) => n.unsigned_abs() <= MAX_SAFE_INTEGER,
        (None, None) => true,
    }
}

/// Trimmed and in canonical form (NFC): the same word typed on two systems
/// is stored the same way.
fn text_value(key: &str, text: &str) -> Result<String, String> {
    let text = canonical(text.trim());
    let text = text.as_str();
    if text.is_empty() {
        return Err(format!(
            "« {key} » : valeur vide (retirez plutôt l’attribut)"
        ));
    }
    if text.chars().count() > MAX_ATTRIBUTE_TEXT_CHARS {
        return Err(format!(
            "« {key} » : {MAX_ATTRIBUTE_TEXT_CHARS} caractères au plus"
        ));
    }
    // One visible line: a list is edited one item per line.
    if text.chars().any(is_hidden) {
        return Err(format!(
            "« {key} » : caractère invisible ou de contrôle interdit (retour à la ligne, \
             tabulation, espace sans largeur…)"
        ));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn map(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => unreachable!("an object"),
        }
    }

    #[test]
    fn keeps_short_typed_values() {
        let given = map(json!({
            "department": "  administratif ",
            "site": ["Lille", " Arras"],
            "seniority": 4,
            "maxId": MAX_SAFE_INTEGER,
            "rate": 0.5,
            "onCall": true,
        }));
        assert_eq!(
            normalize_attributes(&given),
            Ok(map(json!({
                "department": "administratif",
                "site": ["Lille", "Arras"],
                "seniority": 4,
                "maxId": MAX_SAFE_INTEGER,
                "rate": 0.5,
                "onCall": true,
            })))
        );
        assert_eq!(normalize_attributes(&Map::new()), Ok(Map::new()));
        // Decomposed « é » (e + combining acute) is stored composed.
        assert_eq!(
            normalize_attributes(&map(json!({ "site": "Ze\u{301}lande" }))),
            Ok(map(json!({ "site": "Z\u{e9}lande" })))
        );
    }

    #[test]
    fn refuses_keys_a_condition_could_not_read_and_untyped_values() {
        let long_key = format!("a{}", "b".repeat(MAX_ATTRIBUTE_KEY_CHARS));
        let long_text = "x".repeat(MAX_ATTRIBUTE_TEXT_CHARS + 1);
        let too_many: Map<String, Value> = (0..=MAX_ATTRIBUTES)
            .map(|i| (format!("key{i}"), json!("x")))
            .collect();
        let refused = [
            json!({ "Department": "x" }),
            json!({ "1site": "x" }),
            json!({ "site.name": "x" }),
            json!({ "contract_type": "x" }),
            json!({ "": "x" }),
            json!({ long_key: "x" }),
            json!({ "department": "  " }),
            json!({ "department": long_text }),
            json!({ "department": "a\u{0}b" }),
            json!({ "sites": ["Lille\nArras"] }),
            json!({ "department": "admini\u{200B}stratif" }),
            json!({ "department": "a\u{2028}b" }),
            json!({ "department": "a\u{2029}b" }),
            json!({ "department": "admini\u{034F}stratif" }),
            json!({ "department": "admini\u{FE0F}stratif" }),
            json!({ "department": "\u{3164}" }),
            json!({ "department": "\u{202E}abc" }),
            json!({ "matricule": 9_007_199_254_740_992_u64 }),
            json!({ "matricule": -9_007_199_254_740_992_i64 }),
            json!({ "department": null }),
            json!({ "department": { "name": "x" } }),
            json!({ "sites": [] }),
            json!({ "sites": [1, 2] }),
            json!({ "sites": [["Lille"]] }),
            json!({ "sites": vec!["x"; MAX_ATTRIBUTE_LIST_ITEMS + 1] }),
            Value::Object(too_many),
        ];
        for value in refused {
            assert!(
                normalize_attributes(&map(value.clone())).is_err(),
                "{value}"
            );
        }
    }
}
