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
    Ok(String::deserialize(deserializer)?.trim().to_string())
}

/// Same as `trimmed`, for optional fields (combine with `#[serde(default)]`).
pub fn trimmed_opt<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.map(|s| s.trim().to_string()))
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
