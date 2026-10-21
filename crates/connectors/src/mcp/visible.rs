//! What a server declares for the model, hiding nothing (ADR-058): its
//! titles and documentation texts lose their hidden characters (zero-width
//! spaces, direction overrides, tag characters…), which could carry text an
//! administrator never sees and the model reads. What the server itself
//! reads back (a property name, an allowed value) cannot be changed: a
//! schema hiding characters there is refused.

use mimorya_core::plain_text::{shows_all_it_holds, without_hidden};
use serde_json::Value;

/// A title as shown: one line, without hidden characters.
pub fn one_line(text: &str) -> String {
    without_hidden(text).replace(['\n', '\t'], " ")
}

/// A tool's input schema as the model may read it: its documentation
/// (`description` and `title` texts) without hidden characters. Anything
/// else hiding some makes it `None`: taking them out would change what the
/// server receives.
pub fn visible_schema(value: Value) -> Option<Value> {
    match value {
        Value::String(text) => shows_all_it_holds(&text).then_some(Value::String(text)),
        Value::Array(items) => items
            .into_iter()
            .map(visible_schema)
            .collect::<Option<_>>()
            .map(Value::Array),
        Value::Object(fields) => fields
            .into_iter()
            .map(|(key, value)| {
                if !shows_all_it_holds(&key) {
                    return None;
                }
                let value = match (key.as_str(), value) {
                    ("description" | "title", Value::String(text)) => {
                        Value::String(without_hidden(&text))
                    }
                    (_, value) => visible_schema(value)?,
                };
                Some((key, value))
            })
            .collect::<Option<_>>()
            .map(Value::Object),
        other => Some(other),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_title_is_one_visible_line() {
        assert_eq!(
            one_line("Recherche\u{200b}\nde\tprocédures"),
            "Recherche de procédures"
        );
    }

    #[test]
    fn documentation_loses_its_hidden_characters_at_any_depth() {
        let schema = json!({
            "type": "object",
            "description": "Recherche\u{e0049}\u{e0047}\u{e004e}",
            "properties": {
                "query": {
                    "type": "string",
                    "title": "Requête\u{202e}",
                    "description": "Mots-clés\u{200b}.\nExemple :\tcirculateur",
                    "enum": ["circulateur", "chaudière"],
                },
                // A property named like documentation keeps its schema.
                "description": { "type": "string", "maxLength": 80 },
            },
        });
        let visible = visible_schema(schema).unwrap();
        assert_eq!(visible["description"], "Recherche");
        let query = &visible["properties"]["query"];
        assert_eq!(query["title"], "Requête");
        assert_eq!(query["description"], "Mots-clés.\nExemple :\tcirculateur");
        assert_eq!(query["enum"], json!(["circulateur", "chaudière"]));
        assert_eq!(visible["properties"]["description"]["maxLength"], 80);
    }

    #[test]
    fn a_schema_hiding_characters_the_server_reads_back_is_refused() {
        for hiding in [
            json!({ "properties": { "que\u{200b}ry": { "type": "string" } } }),
            json!({ "properties": { "kind": { "enum": ["a", "b\u{e0041}"] } } }),
            json!({ "properties": { "kind": { "default": "x\u{202e}" } } }),
        ] {
            assert_eq!(visible_schema(hiding.clone()), None, "{hiding}");
        }
    }
}
