//! The iCalendar text of an event the platform creates in a CalDAV agenda
//! (ADR-205, RFC 5545), written by the platform from what the person
//! validated: each value escaped, so that nothing the model wrote adds a
//! property or a component; its start and end as instants in UTC, so that
//! it lasts what was validated even across a change of time; lines folded
//! at 75 octets. Attendees are written with the organizer who invites them,
//! for the server to send the invitations (RFC 6638).

use chrono::{DateTime, Utc};

use crate::delegated::{NewEvent, is_plain_address};

/// The longest line, in octets, its line break aside (RFC 5545, 3.1).
const LINE_OCTETS: usize = 75;

/// What an event is written with besides what was validated: its
/// identifier, when it is written, and when it starts and ends.
pub struct Stamped<'a> {
    pub uid: &'a str,
    pub stamp: DateTime<Utc>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Whether `address` is written in a `mailto:` address as it is (RFC
/// 6068): a plain address whose characters a URI never reads otherwise. A
/// `?`, `#` or `%` would start a query, a fragment or an escape, and the
/// server could send the invitation to another address than the one
/// validated.
pub fn written_as_is(address: &str) -> bool {
    let Some((local, _)) = address.split_once('@') else {
        return false;
    };
    is_plain_address(address)
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._~!$'*+".contains(c))
}

/// The calendar object of `event`, invitations from `organizer` when it
/// names one: the caller gives one exactly when there are attendees.
pub fn calendar_object(event: &NewEvent, stamped: &Stamped<'_>, organizer: Option<&str>) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//Mimorya//Agenda//FR".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{}", stamped.uid),
        format!("DTSTAMP:{}", instant(stamped.stamp)),
        format!("DTSTART:{}", instant(stamped.start)),
        format!("DTEND:{}", instant(stamped.end)),
        format!("SUMMARY:{}", text(&event.subject)),
    ];
    if let Some(location) = &event.location {
        lines.push(format!("LOCATION:{}", text(location)));
    }
    if let Some(body) = &event.body {
        lines.push(format!("DESCRIPTION:{}", text(body)));
    }
    if let Some(organizer) = organizer {
        lines.push(format!("ORGANIZER:mailto:{organizer}"));
        lines.extend(event.attendees.iter().map(|attendee| {
            format!(
                "ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:{attendee}"
            )
        }));
    }
    lines.extend(["END:VEVENT".into(), "END:VCALENDAR".into()]);
    lines.iter().map(|line| folded(line)).collect()
}

/// An instant in UTC, as iCalendar writes it.
fn instant(at: DateTime<Utc>) -> String {
    at.format("%Y%m%dT%H%M%SZ").to_string()
}

/// A text value, escaped (RFC 5545, 3.3.11): its line breaks written
/// `\n`, nothing in it ending the value or the line.
fn text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            ';' => escaped.push_str("\\;"),
            ',' => escaped.push_str("\\,"),
            '\n' => escaped.push_str("\\n"),
            // The validation lets no other line break through; none passes.
            '\r' => {}
            c => escaped.push(c),
        }
    }
    escaped
}

/// A line folded at 75 octets, never inside a character, and its break.
fn folded(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + 8);
    let mut width = 0;
    for c in line.chars() {
        if width + c.len_utf8() > LINE_OCTETS {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += c.len_utf8();
    }
    out.push_str("\r\n");
    out
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;
    use crate::standard_mail::caldav::ical::events;
    use crate::standard_mail::caldav::{Person, When};

    fn validated(body: &str) -> NewEvent {
        serde_json::from_value(json!({
            "subject": "Point chantier; lot B, révisé \\ final",
            "start": "2026-11-28T23:30:00",
            "durationMinutes": 90,
            "location": "Quai B",
            "body": body,
            "attendees": ["claire@exemple.fr", "l.martin+chantier@exemple.fr"],
        }))
        .unwrap()
    }

    fn stamped() -> Stamped<'static> {
        let (start, end) = validated("x").instants().unwrap();
        Stamped {
            uid: "4b2d6f0e9a1c4e3f8d7b6a5c4e3f2a1b",
            stamp: Utc.with_ymd_and_hms(2026, 11, 15, 8, 0, 0).unwrap(),
            start,
            end,
        }
    }

    #[test]
    fn an_event_is_written_as_validated_and_read_back_so() {
        let body = "Ordre du jour :\n\tsécurité ; accès, badges\nEND:VEVENT\nBEGIN:VEVENT";
        let written = calendar_object(&validated(body), &stamped(), Some("paul@exemple.fr"));
        assert!(
            written.contains("\r\nDTSTART:20261128T223000Z\r\n"),
            "{written}"
        );
        assert!(
            written.contains("\r\nDTEND:20261129T000000Z\r\n"),
            "{written}"
        );
        assert!(
            written.contains("\r\nSUMMARY:Point chantier\\; lot B\\, révisé \\\\ final\r\n"),
            "{written}"
        );
        // One event, whatever its text says.
        assert_eq!(written.matches("BEGIN:VEVENT").count(), 1, "{written}");
        assert_eq!(
            written.matches("\r\nEND:VEVENT\r\n").count(),
            1,
            "{written}"
        );
        let read = events(&written);
        assert_eq!(read.len(), 1);
        let event = &read[0];
        assert_eq!(
            event.summary.as_deref(),
            Some("Point chantier; lot B, révisé \\ final")
        );
        assert_eq!(event.description.as_deref(), Some(body));
        assert_eq!(event.location.as_deref(), Some("Quai B"));
        assert_eq!(event.start, Some(When::At(stamped().start)));
        assert_eq!(event.end, Some(When::At(stamped().end)));
        let address = |who: &Person| who.address.clone().unwrap_or_default();
        assert_eq!(
            event.organizer.as_ref().map(address).as_deref(),
            Some("paul@exemple.fr")
        );
        let attendees: Vec<String> = event.attendees.iter().map(address).collect();
        assert_eq!(
            attendees,
            ["claire@exemple.fr", "l.martin+chantier@exemple.fr"]
        );
    }

    #[test]
    fn without_an_organizer_no_one_is_invited() {
        let written = calendar_object(&validated("x"), &stamped(), None);
        assert!(
            !written.contains("ORGANIZER") && !written.contains("ATTENDEE"),
            "{written}"
        );
    }

    #[test]
    fn long_lines_are_folded_between_characters() {
        let body = "é".repeat(200);
        let written = calendar_object(&validated(&body), &stamped(), None);
        assert!(written.ends_with("END:VCALENDAR\r\n"));
        for line in written.split("\r\n") {
            assert!(line.len() <= LINE_OCTETS, "{line}");
        }
        assert_eq!(
            events(&written)[0].description.as_deref(),
            Some(body.as_str())
        );
    }

    #[test]
    fn an_address_a_uri_reads_otherwise_is_not_written() {
        for plain in [
            "claire@exemple.fr",
            "l.martin+chantier@exemple.fr",
            "o'neil@exemple.fr",
        ] {
            assert!(written_as_is(plain), "{plain}");
        }
        for read_otherwise in [
            "a?cc=direction@exemple.fr",
            "a#b@exemple.fr",
            "a%40b@exemple.fr",
            "a&b=c@exemple.fr",
            "pas une adresse",
        ] {
            assert!(!written_as_is(read_otherwise), "{read_otherwise}");
        }
    }
}
