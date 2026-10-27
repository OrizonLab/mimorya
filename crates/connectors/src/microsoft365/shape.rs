//! What the tools keep of Graph's answers (ADR-117): a few fields, named in
//! French, each bounded, so that a mailbox's content stays within what a
//! run reads. Dates the person gives are days of the platform's zone (the
//! one the model is told today's date in).

use chrono::{DateTime, Days, NaiveDate, TimeZone, Utc};
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use serde_json::{Value, json};

/// Graph's header asking dates of events in the platform's zone.
pub fn in_platform_zone() -> String {
    format!("outlook.timezone=\"{}\"", PLATFORM_TIME_ZONE.name())
}
/// The most of a short text kept (subject, place, name).
const MAX_SHORT_CHARS: usize = 300;

/// A string field of `value`, bounded, or `null`.
pub fn short(value: &Value, pointer: &str) -> Value {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map_or(Value::Null, |text| json!(bounded(text, MAX_SHORT_CHARS).0))
}

/// The text cut at `max` characters, and whether it was.
pub fn bounded(text: &str, max: usize) -> (String, bool) {
    match text.char_indices().nth(max) {
        Some((at, _)) => (text[..at].to_string(), true),
        None => (text.to_string(), false),
    }
}

/// "Nom <adresse>" of a Graph recipient (`{ emailAddress: { name, address } }`).
pub fn person(recipient: &Value) -> Value {
    let name = short(recipient, "/emailAddress/name");
    let address = short(recipient, "/emailAddress/address");
    match (name.as_str(), address.as_str()) {
        (Some(name), Some(address)) if name != address => json!(format!("{name} <{address}>")),
        (_, Some(address)) => json!(address),
        (Some(name), None) => json!(name),
        (None, None) => Value::Null,
    }
}

/// The people of a list of recipients, at most `max`, and how many more.
pub fn people(recipients: Option<&Value>, max: usize) -> (Vec<Value>, usize) {
    let all = recipients
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let kept = all.iter().take(max).map(person).collect();
    (kept, all.len().saturating_sub(max))
}

/// The instant a day of the platform's zone starts, in UTC.
pub fn day_start(day: NaiveDate) -> DateTime<Utc> {
    let midnight = day.and_hms_opt(0, 0, 0).unwrap_or_default();
    // A day always has a first instant, even where midnight is skipped.
    PLATFORM_TIME_ZONE
        .from_local_datetime(&midnight)
        .earliest()
        .map_or_else(
            || Utc.from_utc_datetime(&midnight),
            |at| at.with_timezone(&Utc),
        )
}

/// The instant the day after `day` starts: the end of a period that
/// includes `day`.
pub fn day_end(day: NaiveDate) -> DateTime<Utc> {
    day_start(day.checked_add_days(Days::new(1)).unwrap_or(day))
}

/// Graph's form of an instant.
pub fn graph_instant(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// The items of a Graph collection, and whether Graph has more.
pub fn collection(answer: &Value) -> (&[Value], bool) {
    let items = answer
        .get("value")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    (items, answer.get("@odata.nextLink").is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_start_in_the_platform_zone() {
        let winter = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        assert_eq!(graph_instant(day_start(winter)), "2026-01-14T23:00:00Z");
        let summer = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap();
        assert_eq!(graph_instant(day_end(summer)), "2026-07-15T22:00:00Z");
    }

    #[test]
    fn texts_are_cut_on_characters() {
        assert_eq!(bounded("éèà", 2), ("éè".to_string(), true));
        assert_eq!(bounded("éèà", 3), ("éèà".to_string(), false));
    }

    #[test]
    fn a_recipient_reads_as_name_and_address() {
        let named =
            json!({ "emailAddress": { "name": "Léa Martin", "address": "lea@exemple.fr" } });
        assert_eq!(person(&named), json!("Léa Martin <lea@exemple.fr>"));
        let bare = json!({ "emailAddress": { "address": "lea@exemple.fr" } });
        assert_eq!(person(&bare), json!("lea@exemple.fr"));
        let list = json!([named, bare, bare]);
        let (kept, more) = people(Some(&list), 2);
        assert_eq!((kept.len(), more), (2, 1));
    }
}
