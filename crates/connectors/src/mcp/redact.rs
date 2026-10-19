//! The access token of an MCP server reaches neither the logs, nor the
//! administrators, nor the model: whatever the server sends back (an error
//! quoting the request, a tool result) is masked before leaving the connector.

use std::fmt;
use std::sync::Arc;

use serde_json::Value;

pub const MASK: &str = "***";

/// Masks one secret wherever it appears. No `Debug` of the secret.
#[derive(Clone, Default)]
pub struct Redactor(Option<Arc<str>>);

impl Redactor {
    pub fn new(secret: Option<&str>) -> Self {
        Self(secret.filter(|s| !s.is_empty()).map(Arc::from))
    }

    pub fn text(&self, text: &str) -> String {
        match &self.0 {
            Some(secret) => text.replace(&**secret, MASK),
            None => text.to_string(),
        }
    }

    /// Every string of the value, keys included.
    pub fn value(&self, value: Value) -> Value {
        if self.0.is_none() {
            return value;
        }
        match value {
            Value::String(text) => Value::String(self.text(&text)),
            Value::Array(items) => Value::Array(items.into_iter().map(|v| self.value(v)).collect()),
            Value::Object(fields) => Value::Object(
                fields
                    .into_iter()
                    .map(|(key, v)| (self.text(&key), self.value(v)))
                    .collect(),
            ),
            other => other,
        }
    }
}

impl fmt::Debug for Redactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Redactor")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn masks_the_secret_in_text_and_in_every_string_of_a_value() {
        let redactor = Redactor::new(Some("s3cr\"et"));
        assert_eq!(redactor.text("Bearer s3cr\"et!"), "Bearer ***!");
        let value = json!({ "a": ["x s3cr\"et", 1], "s3cr\"et": { "b": "s3cr\"et" } });
        assert_eq!(
            redactor.value(value),
            json!({ "a": ["x ***", 1], "***": { "b": "***" } })
        );
        assert_eq!(Redactor::new(Some("")).text("abc"), "abc");
        assert_eq!(Redactor::new(None).value(json!("abc")), json!("abc"));
        assert_eq!(format!("{redactor:?}"), "Redactor");
    }
}
