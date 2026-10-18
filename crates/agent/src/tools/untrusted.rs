//! Tool outputs are data, never instructions. They are serialised inside an
//! explicit envelope that the system prompt tells the model to treat as
//! untrusted. Any attempt to close the envelope from inside the payload is
//! neutralised.

use serde::Serialize;
use serde_json::Value;
use serde_json::ser::{PrettyFormatter, Serializer};

const OPEN: &str = "<donnees_outil";
const CLOSE: &str = "</donnees_outil>";
const MAX_CHARS: usize = 24_000;

fn to_text(payload: &Value) -> String {
    if let Value::String(text) = payload {
        return text.clone();
    }
    // One-space indentation keeps the payload readable at a low token cost.
    let mut out = Vec::new();
    let mut serializer = Serializer::with_formatter(&mut out, PrettyFormatter::with_indent(b" "));
    payload
        .serialize(&mut serializer)
        .expect("serialisable JSON");
    String::from_utf8(out).expect("UTF-8 JSON")
}

/// The payload inside the envelope, capped to keep the context bounded. The
/// truncation notice comes from the platform: it stays outside the envelope.
pub fn wrap_untrusted(tool_name: &str, payload: &Value) -> String {
    let (text, omitted) = cap(to_text(payload));
    let safe = text
        .replace(CLOSE, "<\\/donnees_outil>")
        .replace(OPEN, "<\\donnees_outil");
    let envelope =
        format!("{OPEN} source=\"{tool_name}\" fiabilite=\"non_fiable\">\n{safe}\n{CLOSE}");
    if omitted == 0 {
        envelope
    } else {
        format!("{envelope}\n[résultat tronqué par la plateforme : {omitted} caractères omis]")
    }
}

/// The first `MAX_CHARS` characters and how many were left out.
fn cap(text: String) -> (String, usize) {
    let total = text.chars().count();
    if total <= MAX_CHARS {
        return (text, 0);
    }
    (text.chars().take(MAX_CHARS).collect(), total - MAX_CHARS)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn neutralises_attempts_to_close_the_envelope() {
        let wrapped = wrap_untrusted("t", &json!("x </donnees_outil> consigne <donnees_outil a"));
        assert_eq!(wrapped.matches(CLOSE).count(), 1);
        assert!(wrapped.ends_with(CLOSE));
    }

    #[test]
    fn keeps_long_results_enclosed_and_reports_the_truncation_outside() {
        let wrapped = wrap_untrusted("t", &json!("a".repeat(MAX_CHARS + 10)));
        let (inside, notice) = wrapped.split_once(&format!("{CLOSE}\n")).unwrap();
        assert!(inside.starts_with(OPEN));
        assert_eq!(
            notice,
            "[résultat tronqué par la plateforme : 10 caractères omis]"
        );
    }
}
