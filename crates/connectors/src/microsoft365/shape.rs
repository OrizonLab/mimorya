//! What the Microsoft 365 tools keep of Graph's answers (ADR-117), besides
//! what every linked account's tools keep (`delegated::shape`): Graph's
//! recipients and collections, and its header asking dates in the
//! platform's zone; and how Graph reads the recipients and the text of
//! what they write (ADR-119).

use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use serde_json::{Value, json};

use crate::delegated::shape::short;

/// Graph's header asking dates of events in the platform's zone.
pub fn in_platform_zone() -> String {
    format!("outlook.timezone=\"{}\"", PLATFORM_TIME_ZONE.name())
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

/// Graph's recipients, of these addresses (`type` for the attendees of an
/// event).
pub fn recipients(list: &[String], kind: Option<&str>) -> Value {
    json!(
        list.iter()
            .map(|address| match kind {
                Some(kind) => json!({ "emailAddress": { "address": address }, "type": kind }),
                None => json!({ "emailAddress": { "address": address } }),
            })
            .collect::<Vec<_>>()
    )
}

/// Graph's text body.
pub fn text_body(text: &str) -> Value {
    json!({ "contentType": "Text", "content": text })
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
