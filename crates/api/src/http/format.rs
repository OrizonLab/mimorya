//! Wire formats shared by the routes: timestamps as the web client expects
//! them (ISO 8601, milliseconds, UTC) and trimmed strings in request bodies.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// e.g. `2026-10-18T09:30:00.000Z`.
pub fn iso(instant: &DateTime<Utc>) -> String {
    instant.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn iso_opt(instant: &Option<DateTime<Utc>>) -> Value {
    instant
        .as_ref()
        .map_or(Value::Null, |i| Value::String(iso(i)))
}

/// `deserialize_with` helper: surrounding whitespace is removed before validation.
pub fn trimmed<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    storable::<D>(String::deserialize(deserializer)?.trim().to_string())
}

/// PostgreSQL cannot store a NUL character: refused as invalid input (400)
/// rather than failing at the database (500).
fn storable<'de, D: Deserializer<'de>>(text: String) -> Result<String, D::Error> {
    if text.contains('\0') {
        return Err(serde::de::Error::custom("caractère nul interdit"));
    }
    Ok(text)
}

/// Same as `trimmed`, for optional fields (combine with `#[serde(default)]`).
pub fn trimmed_opt<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)?
        .map(|s| storable::<D>(s.trim().to_string()))
        .transpose()
}

/// `deserialize_with` helper for fields that can be cleared: absent → `None`,
/// `null` or blank → `Some(None)`, text → `Some(Some(trimmed))`. Combine with
/// `#[serde(default)]`.
pub fn trimmed_nullable<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    let value = Option::<String>::deserialize(deserializer)?
        .map(|s| storable::<D>(s.trim().to_string()))
        .transpose()?;
    Ok(Some(value.filter(|s| !s.is_empty())))
}

/// `deserialize_with` helper: absent → `None`, `null` → `Some(None)`.
pub fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn formats_timestamps_with_milliseconds() {
        let instant = Utc.with_ymd_and_hms(2026, 10, 18, 9, 30, 0).unwrap();
        assert_eq!(iso(&instant), "2026-10-18T09:30:00.000Z");
        assert_eq!(iso_opt(&None), Value::Null);
    }
}
