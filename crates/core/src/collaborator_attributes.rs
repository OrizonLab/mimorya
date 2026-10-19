//! Rules for the attributes of a collaborator (`users.attributes`), which
//! access policy conditions read as `subject.attributes.<key>` (ADR-023). An
//! administrator sets them; values stay short and typed, so that a condition
//! compares them reliably and nothing sensitive lands there by accident.

use serde_json::{Map, Value};

pub const MAX_ATTRIBUTES: usize = 20;
pub const MAX_ATTRIBUTE_KEY_CHARS: usize = 40;
pub const MAX_ATTRIBUTE_TEXT_CHARS: usize = 200;
pub const MAX_ATTRIBUTE_LIST_ITEMS: usize = 20;

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

fn text_value(key: &str, text: &str) -> Result<String, String> {
    let text = text.trim();
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
    // One line of text: a list is edited one item per line.
    if text.chars().any(char::is_control) {
        return Err(format!(
            "« {key} » : caractère de contrôle interdit (retour à la ligne, tabulation…)"
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
            "onCall": true,
        }));
        assert_eq!(
            normalize_attributes(&given),
            Ok(map(json!({
                "department": "administratif",
                "site": ["Lille", "Arras"],
                "seniority": 4,
                "onCall": true,
            })))
        );
        assert_eq!(normalize_attributes(&Map::new()), Ok(Map::new()));
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
