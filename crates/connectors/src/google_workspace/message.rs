//! What the Gmail tools read of an e-mail Gmail gives (ADR-122): when it
//! was received, its headers, bounded, where the person opens it, and its
//! citation, named by its subject.

use chrono::DateTime;
use mimorya_core::tools::Citation;
use serde_json::{Value, json};

use super::mime::header;
use crate::delegated::shape::{bounded, utc_instant};

/// When Gmail received an e-mail (`internalDate`, milliseconds).
pub fn received(message: &Value) -> Value {
    message
        .get("internalDate")
        .and_then(Value::as_str)
        .and_then(|millis| millis.parse::<i64>().ok())
        .and_then(DateTime::from_timestamp_millis)
        .map_or(Value::Null, |at| json!(utc_instant(at)))
}

/// A header of the e-mail, bounded.
pub fn header_of(message: &Value, name: &str, max: usize) -> Value {
    message
        .get("payload")
        .and_then(|payload| header(payload, name))
        .map_or(Value::Null, |value| json!(bounded(value, max).0))
}

/// Where the person opens the e-mail in Gmail.
pub fn link(id: &str) -> String {
    format!("https://mail.google.com/mail/#all/{id}")
}

pub fn citation(message: &Value) -> Option<Citation> {
    let id = message.get("id").and_then(Value::as_str)?;
    let subject = message
        .get("payload")
        .and_then(|payload| header(payload, "Subject"))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("(sans objet)");
    Some(Citation::new("email", id).labelled(bounded(subject, 120).0))
}
