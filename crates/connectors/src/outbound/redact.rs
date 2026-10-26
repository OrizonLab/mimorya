//! The access token of an MCP server reaches neither the logs, nor the
//! administrators, nor the model: whatever the server sends back (an error
//! quoting the request, a tool result) is masked before leaving the connector,
//! as is, escaped in JSON (`\/`, `\u002B`…) or encoded in a URL (`%2F`…), each
//! character of `/ + = " \` written one way throughout (ADR-026). HTML
//! entities, double escaping and mixed spellings are not covered.

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
pub struct Redactor(Option<Arc<Spellings>>);

struct Spellings {
    /// The longest run of the secret without an escaped character: every
    /// spelling holds it, so a text without it is read once, not once per
    /// spelling.
    core: String,
    all: Vec<String>,
}

impl Spellings {
    /// The spellings the text may hold.
    fn within<'a>(&'a self, text: &'a str) -> impl Iterator<Item = &'a String> {
        let possible = text.contains(self.core.as_str());
        self.all
            .iter()
            .filter(move |spelling| possible && text.contains(spelling.as_str()))
    }
}

impl Redactor {
    pub fn new(secret: Option<&str>) -> Self {
        Self(secret.filter(|s| !s.is_empty()).map(|secret| {
            let core = secret
                .split(|c| ESCAPED.contains(&c))
                .max_by_key(|run| run.len())
                .unwrap_or_default();
            Arc::new(Spellings {
                core: core.to_string(),
                all: spellings(secret),
            })
        }))
    }

    /// How long the longest spelling of the secret is, in bytes (none without
    /// a secret): a text whose last this many bytes are dropped after it is
    /// masked holds no part of the secret, even if it was cut where the
    /// secret began.
    pub fn longest(&self) -> usize {
        self.0
            .as_ref()
            .and_then(|spellings| spellings.all.first())
            .map_or(0, String::len)
    }

    /// Whether the text quotes the secret.
    pub fn reveals(&self, text: &str) -> bool {
        self.0
            .as_ref()
            .is_some_and(|spellings| spellings.within(text).next().is_some())
    }

    pub fn text(&self, text: &str) -> String {
        let mut masked = text.to_string();
        let Some(spellings) = &self.0 else {
            return masked;
        };
        if !text.contains(spellings.core.as_str()) {
            return masked;
        }
        for spelling in &spellings.all {
            if masked.contains(spelling.as_str()) {
                masked = masked.replace(spelling.as_str(), MASK);
            }
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
    fn knows_how_long_the_secret_may_be_written() {
        assert_eq!(Redactor::new(Some("ab/cd")).longest(), "ab\\u002Fcd".len());
        assert_eq!(Redactor::new(Some("abcd")).longest(), 4);
        assert_eq!(Redactor::default().longest(), 0);
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
            "ab%2fcd%2bef%3d%3d",
        ] {
            let text = format!("Bearer {written} refusé");
            assert_eq!(redactor.text(&text), "Bearer *** refusé", "{written}");
            assert!(redactor.reveals(&text), "{written}");
        }
        assert!(!redactor.reveals("ab cd ef"));
        // Made of escaped characters only: no shortcut, still masked.
        let only = Redactor::new(Some("+/+/"));
        assert_eq!(only.text("x %2B%2F%2B%2F y"), "x *** y");
        // A plain token has one spelling.
        assert_eq!(spellings("abcdef0123"), ["abcdef0123"]);
    }
}
