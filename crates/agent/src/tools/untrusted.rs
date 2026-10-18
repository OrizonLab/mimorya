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

pub fn wrap_untrusted(tool_name: &str, payload: &Value) -> String {
    let safe = to_text(payload)
        .replace(CLOSE, "<\\/donnees_outil>")
        .replace(OPEN, "<\\donnees_outil");
    format!("{OPEN} source=\"{tool_name}\" fiabilite=\"non_fiable\">\n{safe}\n{CLOSE}")
}

/// Caps a tool payload to keep the context bounded.
pub fn truncate(text: String) -> String {
    let total = text.chars().count();
    if total <= MAX_CHARS {
        return text;
    }
    let kept: String = text.chars().take(MAX_CHARS).collect();
    format!(
        "{kept}\n… [résultat tronqué : {} caractères omis]",
        total - MAX_CHARS
    )
}
