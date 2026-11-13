//! The events of an iCalendar text (RFC 5545) as the agenda tool reads
//! them (ADR-205): its lines unfolded, each `VEVENT`'s summary, start and
//! end (a day, an instant, a zone's time or a floating time), location,
//! description, status, organizer and attendees, its text values
//! unescaped; whatever else it holds (alarms, zones, unknown properties)
//! left aside. What a server sends is bounded before it reaches here; each
//! value and the number of events are bounded again.

use chrono::Duration;

pub use super::time::When;
use super::time::{duration, when};

/// The most events read from one text.
pub const MAX_EVENTS: usize = 500;
/// The most attendees kept of one event.
const MAX_ATTENDEES: usize = 50;
/// The most characters kept of one value.
const MAX_VALUE_CHARS: usize = 4_000;

/// Someone an event names: their name, their address.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Person {
    pub name: Option<String>,
    pub address: Option<String>,
}

/// An event as read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Event {
    pub uid: Option<String>,
    /// The occurrence it is, for one of a recurring event.
    pub occurrence: Option<String>,
    pub summary: Option<String>,
    pub start: Option<When>,
    pub end: Option<When>,
    /// Its length, when it gives one instead of an end.
    pub duration: Option<Duration>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub cancelled: bool,
    /// It repeats, and the server gave it unexpanded.
    pub repeats: bool,
    pub organizer: Option<Person>,
    pub attendees: Vec<Person>,
}

/// One content line: its name, its parameters, its value.
struct Line<'a> {
    name: String,
    params: Vec<(String, String)>,
    value: &'a str,
}

impl Line<'_> {
    fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// The events of `text`, at most `MAX_EVENTS`.
pub fn events(text: &str) -> Vec<Event> {
    let unfolded = unfold(text);
    let mut events = Vec::new();
    let mut current: Option<Event> = None;
    // Components nested in an event (an alarm) are skipped whole.
    let mut nested = 0usize;
    for raw in unfolded.split('\n') {
        let Some(line) = line(raw.trim_end_matches('\r')) else {
            continue;
        };
        let value = line.value.trim();
        match (line.name.as_str(), &mut current) {
            ("BEGIN", None) if value.eq_ignore_ascii_case("VEVENT") => {
                current = Some(Event::default());
            }
            ("BEGIN", Some(_)) => nested += 1,
            ("END", Some(_)) if nested > 0 => nested -= 1,
            ("END", Some(_)) if value.eq_ignore_ascii_case("VEVENT") => {
                events.extend(current.take());
                if events.len() == MAX_EVENTS {
                    break;
                }
            }
            (_, Some(event)) if nested == 0 => read_property(event, &line),
            _ => {}
        }
    }
    events
}

/// Lines joined back where a line break followed by a space or a tab
/// folded them (RFC 5545, 3.1).
fn unfold(text: &str) -> String {
    text.replace("\r\n ", "")
        .replace("\r\n\t", "")
        .replace("\n ", "")
        .replace("\n\t", "")
}

/// A line's name, parameters and value; none for a line without a colon.
fn line(raw: &str) -> Option<Line<'_>> {
    let mut quoted = false;
    let colon = raw.char_indices().find_map(|(at, c)| match c {
        '"' => {
            quoted = !quoted;
            None
        }
        ':' if !quoted => Some(at),
        _ => None,
    })?;
    let (head, value) = (&raw[..colon], &raw[colon + 1..]);
    let mut parts = split_unquoted(head, ';').into_iter();
    let name = parts.next()?.trim().to_ascii_uppercase();
    let params = parts
        .filter_map(|param| {
            let (key, value) = param.split_once('=')?;
            Some((
                key.trim().to_string(),
                value.trim().trim_matches('"').to_string(),
            ))
        })
        .collect();
    Some(Line {
        name,
        params,
        value,
    })
}

fn split_unquoted(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut quoted, mut start) = (false, 0);
    for (at, c) in text.char_indices() {
        match c {
            '"' => quoted = !quoted,
            c if c == separator && !quoted => {
                parts.push(&text[start..at]);
                start = at + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

fn read_property(event: &mut Event, line: &Line<'_>) {
    let value = line.value;
    match line.name.as_str() {
        "UID" => event.uid = Some(kept(value)),
        "RECURRENCE-ID" => event.occurrence = Some(kept(value)),
        "SUMMARY" => event.summary = Some(text(value)),
        "LOCATION" => event.location = Some(text(value)),
        "DESCRIPTION" => event.description = Some(text(value)),
        "DTSTART" => event.start = line_when(line),
        "DTEND" => event.end = line_when(line),
        "DURATION" => event.duration = duration(value),
        "STATUS" => event.cancelled = value.trim().eq_ignore_ascii_case("CANCELLED"),
        "RRULE" | "RDATE" => event.repeats = true,
        "ORGANIZER" => event.organizer = Some(person(line)),
        "ATTENDEE" if event.attendees.len() < MAX_ATTENDEES => {
            event.attendees.push(person(line));
        }
        _ => {}
    }
}

/// A value kept as it is, bounded.
fn kept(value: &str) -> String {
    value.trim().chars().take(MAX_VALUE_CHARS).collect()
}

/// A text value unescaped (RFC 5545, 3.3.11), bounded.
fn text(value: &str) -> String {
    let mut out = String::new();
    let mut kept = 0;
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if kept == MAX_VALUE_CHARS {
            break;
        }
        let read = match c {
            '\\' => match chars.next() {
                Some('n' | 'N') => '\n',
                Some(other) => other,
                None => break,
            },
            c => c,
        };
        out.push(read);
        kept += 1;
    }
    out.trim().to_string()
}

/// A start or an end, with its kind and zone.
fn line_when(line: &Line<'_>) -> Option<When> {
    when(line.value, line.param("VALUE"), line.param("TZID"))
}

/// The organizer or an attendee: their common name and their address
/// without `mailto:`.
fn person(line: &Line<'_>) -> Person {
    let value = line.value.trim();
    let address = value
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("mailto:"))
        .map_or(value, |_| &value[7..]);
    Person {
        name: line.param("CN").map(kept).filter(|name| !name.is_empty()),
        address: Some(kept(address)).filter(|address| !address.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, TimeZone, Utc};

    use super::*;

    const AGENDA: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/Paris\r\n\
        END:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:r1@neo\r\nSUMMARY:Réunion de chantier\\, lot 3\r\n\
        DTSTART;TZID=Europe/Paris:20261116T090000\r\nDTEND;TZID=Europe/Paris:20261116T103000\r\n\
        LOCATION:Atelier\\; quai B\r\nDESCRIPTION:Ordre du jour :\\npoint 1\\npoint\r\n  2\r\n\
        ORGANIZER;CN=\"Léa: cheffe\":mailto:lea@neo.example\r\n\
        ATTENDEE;CN=Paul;PARTSTAT=ACCEPTED:MAILTO:paul@neo.example\r\n\
        BEGIN:VALARM\r\nACTION:DISPLAY\r\nSUMMARY:pas l'objet\r\nEND:VALARM\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:j1@neo\r\nSUMMARY:Congés\r\nDTSTART;VALUE=DATE:20261118\r\n\
        DTEND;VALUE=DATE:20261119\r\nSTATUS:CANCELLED\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:u1@neo\r\nDTSTART:20261117T130000Z\r\nDURATION:PT1H30M\r\n\
        RECURRENCE-ID:20261117T130000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn reads_each_event_and_leaves_the_rest() {
        let read = events(AGENDA);
        assert_eq!(read.len(), 3);
        let meeting = &read[0];
        assert_eq!(
            meeting.summary.as_deref(),
            Some("Réunion de chantier, lot 3")
        );
        assert_eq!(meeting.location.as_deref(), Some("Atelier; quai B"));
        assert_eq!(
            meeting.description.as_deref(),
            Some("Ordre du jour :\npoint 1\npoint 2")
        );
        let nine = Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap();
        assert_eq!(meeting.start, Some(When::At(nine)));
        let organizer = meeting.organizer.clone().unwrap();
        assert_eq!(organizer.name.as_deref(), Some("Léa: cheffe"));
        assert_eq!(organizer.address.as_deref(), Some("lea@neo.example"));
        assert_eq!(
            meeting.attendees[0].address.as_deref(),
            Some("paul@neo.example")
        );
        let leave = &read[1];
        let day = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        assert_eq!(leave.start, Some(When::Day(day)));
        assert!(leave.cancelled && leave.repeats);
        let once = &read[2];
        assert_eq!(once.duration, Some(Duration::minutes(90)));
        assert!(once.occurrence.is_some());
    }

    #[test]
    fn an_unknown_zone_reads_as_a_floating_time() {
        let read = events("BEGIN:VEVENT\nDTSTART;TZID=Heure de Mars:20261116T090000\nEND:VEVENT");
        let nine = NaiveDate::from_ymd_opt(2026, 11, 16)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        assert_eq!(read[0].start, Some(When::Floating(nine)));
    }

    #[test]
    fn events_and_values_are_bounded() {
        let one = "BEGIN:VEVENT\nSUMMARY:x\nEND:VEVENT\n";
        assert_eq!(events(&one.repeat(MAX_EVENTS + 10)).len(), MAX_EVENTS);
        let long = format!("BEGIN:VEVENT\nSUMMARY:{}\nEND:VEVENT", "a".repeat(10_000));
        let summary = events(&long)[0].summary.clone().unwrap();
        assert_eq!(summary.chars().count(), MAX_VALUE_CHARS);
    }
}
