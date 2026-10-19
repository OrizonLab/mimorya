//! The access token of an MCP server reaches neither the logs, nor the
//! administrators, nor the model: whatever the server sends back (an error
//! quoting the request, a tool result) is masked before leaving the connector,
//! in every spelling a server may give it back: as is, escaped in JSON
//! (`\/`, `\u002B`…) or encoded in a URL (`%2F`…).

use std::fmt;
use std::sync::Arc;

use serde_json::Value;

pub const MASK: &str = "***";
/// Characters an encoder may write otherwise, the backslash first: the
/// other spellings contain one.
const ESCAPED: [char; 5] = ['\\', '"', '/', '+', '='];
const MAX_SPELLINGS: usize = 1000;

/// Masks one secret wherever it appears. No `Debug` of the secret.
#[derive(Clone, Default)]
pub struct Redactor(Option<Arc<[String]>>);

impl Redactor {
    pub fn new(secret: Option<&str>) -> Self {
        Self(
            secret
                .filter(|s| !s.is_empty())
                .map(|s| spellings(s).into()),
        )
    }

    /// Whether the text quotes the secret.
    pub fn reveals(&self, text: &str) -> bool {
        self.0
            .as_ref()
            .is_some_and(|spellings| spellings.iter().any(|s| text.contains(s.as_str())))
    }

    pub fn text(&self, text: &str) -> String {
        let mut masked = text.to_string();
        for spelling in self.0.iter().flat_map(|s| s.iter()) {
            masked = masked.replace(spelling.as_str(), MASK);
        }
        masked
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

/// How one character may be written back: as is, escaped in JSON, encoded
/// in a URL.
fn forms(c: char) -> Vec<String> {
    let code = u32::from(c);
    let mut forms = vec![
        c.to_string(),
        format!("\\u{code:04X}"),
        format!("\\u{code:04x}"),
        format!("%{code:02X}"),
        format!("%{code:02x}"),
    ];
    if matches!(c, '\\' | '"' | '/') {
        forms.push(format!("\\{c}"));
    }
    forms.dedup();
    forms
}

/// The secret as an encoder may write it back: each kind of character in
/// one form, the longest spellings first.
fn spellings(secret: &str) -> Vec<String> {
    let mut spellings = vec![secret.to_string()];
    for kind in ESCAPED.into_iter().filter(|c| secret.contains(*c)) {
        let forms = forms(kind);
        if spellings.len() * forms.len() > MAX_SPELLINGS {
            break;
        }
        spellings = spellings
            .iter()
            .flat_map(|spelling| forms.iter().map(move |form| spelling.replace(kind, form)))
            .collect();
    }
    spellings.sort_by_key(|s| std::cmp::Reverse(s.len()));
    spellings
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
        assert!(redactor.reveals("x s3cr\"et"));
        assert!(!redactor.reveals("s3cr"));
        assert!(!Redactor::default().reveals("s3cr\"et"));
        let value = json!({ "a": ["x s3cr\"et", 1], "s3cr\"et": { "b": "s3cr\"et" } });
        assert_eq!(
            redactor.value(value),
            json!({ "a": ["x ***", 1], "***": { "b": "***" } })
        );
        assert_eq!(Redactor::new(Some("")).text("abc"), "abc");
        assert_eq!(Redactor::new(None).value(json!("abc")), json!("abc"));
        assert_eq!(format!("{redactor:?}"), "Redactor");
    }

    #[test]
    fn masks_the_secret_however_an_encoder_wrote_it() {
        let redactor = Redactor::new(Some("ab/cd+ef=="));
        for written in [
            "ab/cd+ef==",
            "ab\\/cd+ef==",
            "ab/cd\\u002Bef==",
            "ab\\/cd\\u002bef\\u003D\\u003D",
            "ab%2Fcd%2Bef%3D%3D",
        ] {
            let text = format!("Bearer {written} refusé");
            assert_eq!(redactor.text(&text), "Bearer *** refusé", "{written}");
            assert!(redactor.reveals(&text), "{written}");
        }
        assert!(!redactor.reveals("ab cd ef"));
        // A plain token has one spelling.
        assert_eq!(spellings("abcdef0123"), ["abcdef0123"]);
    }
}
