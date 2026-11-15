//! What an event read from an agenda says to the model (ADR-205): when
//! it starts and ends as instants, a whole day from its start in the
//! platform's zone and a floating time read in it; whether it overlaps the
//! period asked (RFC 4791, 9.9); and its fields, bounded, in Paris time,
//! with the attendees past those shown counted.

use chrono::{DateTime, Utc};
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use mimorya_core::tools::Citation;
use serde_json::{Value, json};

use crate::delegated::shape::{MAX_SHORT_CHARS, bounded, day_start};
use crate::standard_mail::caldav::{Event, Person, When, in_zone};

/// The most attendees told of one event.
const MAX_ATTENDEES: usize = 10;

/// An event read, with the agenda it is in.
pub(super) struct Listed {
    pub agenda: Option<String>,
    pub event: Event,
}

/// An instant of a start or an end: a day from its start in the
/// platform's zone, a floating time read in it.
pub(super) fn instant(when: When) -> DateTime<Utc> {
    match when {
        When::Day(day) => day_start(day),
        When::At(at) => at,
        When::Floating(at) => in_zone(PLATFORM_TIME_ZONE, at).unwrap_or_else(|| at.and_utc()),
    }
}

/// Where an event ends: its end, or its start and length, or the day
/// after a whole day's start; none past what an instant can hold.
fn end_of(event: &Event) -> Option<When> {
    match (event.end, event.start, event.duration) {
        (Some(end), _, _) => Some(end),
        (None, Some(start), Some(length)) => {
            instant(start).checked_add_signed(length).map(When::At)
        }
        (None, Some(When::Day(day)), None) => day.succ_opt().map(When::Day),
        _ => None,
    }
}

/// A recurring event the server gave whole, not its occurrences.
fn unexpanded(event: &Event) -> bool {
    event.repeats && event.occurrence.is_none()
}

/// Whether the event overlaps the period asked, its end excluded as the
/// period's (RFC 4791, 9.9): one that lasts, from its start to its end;
/// one with no length, at its start. A recurring event the server did not
/// expand may recur in it once it has begun.
pub(super) fn overlaps(event: &Event, (from, to): (DateTime<Utc>, DateTime<Utc>)) -> bool {
    let Some(start) = event.start.map(instant) else {
        return false;
    };
    if unexpanded(event) {
        return start < to;
    }
    let end = end_of(event).map(instant).filter(|end| *end > start);
    match end {
        Some(end) => start < to && end > from,
        None => start >= from && start < to,
    }
}

/// A start or an end as the model reads it: a day, or a time of the
/// platform's zone.
fn written(when: When) -> Value {
    match when {
        When::Day(day) => json!(day.format("%Y-%m-%d").to_string()),
        at => json!(instant(at).with_timezone(&PLATFORM_TIME_ZONE).to_rfc3339()),
    }
}

/// "Nom <adresse>".
fn person(who: &Person) -> Value {
    match (who.name.as_deref(), who.address.as_deref()) {
        (Some(name), Some(address)) if name != address => json!(format!("{name} <{address}>")),
        (_, Some(address)) => json!(address),
        (Some(name), None) => json!(name),
        (None, None) => Value::Null,
    }
}

fn short(text: &Option<String>) -> Value {
    text.as_deref()
        .map_or(Value::Null, |text| json!(bounded(text, MAX_SHORT_CHARS).0))
}

pub(super) fn event(listed: &Listed) -> Value {
    let event = &listed.event;
    let all_day = matches!(event.start, Some(When::Day(_)));
    let mut read = json!({
        "id": short(&event.uid),
        "occurrence": short(&event.occurrence),
        "objet": short(&event.summary),
        "debut": event.start.map_or(Value::Null, written),
        // An end that does not follow the start is none, as `overlaps` reads it.
        "fin": end_of(event)
            .filter(|end| event.start.is_some_and(|start| instant(*end) > instant(start)))
            .map_or(Value::Null, written),
        "journeeEntiere": all_day,
        "annule": event.cancelled,
        "lieu": short(&event.location),
        "organisateur": event.organizer.as_ref().map_or(Value::Null, person),
        "participants": event.attendees.iter().take(MAX_ATTENDEES).map(person).collect::<Vec<_>>(),
        "autresParticipants": event.attendee_count.saturating_sub(MAX_ATTENDEES),
        "apercu": short(&event.description),
        "agenda": short(&listed.agenda),
    });
    if unexpanded(event) {
        read["repetition"] = json!(
            "Rendez-vous répétitif dont le serveur n’a pas donné les occurrences : seule la première est datée, il peut se répéter dans la période"
        );
    }
    read
}

pub(super) fn citation(event: &Event) -> Option<Citation> {
    let id = event.uid.as_deref()?;
    let subject = event
        .summary
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("(sans objet)");
    Some(Citation::new("event", bounded(id, 200).0).labelled(bounded(subject, 120).0))
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, TimeZone};

    use super::*;
    use crate::delegated::shape::day_end;

    fn at(day: u32, hour: u32) -> When {
        When::At(Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0).unwrap())
    }

    #[test]
    fn the_period_s_bounds_exclude_what_ends_or_starts_there() {
        let (from, to) = (
            NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 18).unwrap(),
        );
        let period = (day_start(from), day_end(to));
        let day = |d| When::Day(NaiveDate::from_ymd_opt(2026, 11, d).unwrap());
        let event = |start, end| Event {
            start: Some(start),
            end,
            ..Event::default()
        };
        // Whole days before and after, an event ending at the period's
        // start (midnight in Paris is 23:00 UTC), one starting at its end.
        assert!(!overlaps(&event(day(15), None), period));
        assert!(!overlaps(&event(day(19), None), period));
        assert!(!overlaps(&event(at(15, 22), Some(at(15, 23))), period));
        assert!(!overlaps(&event(at(18, 23), Some(at(19, 1))), period));
        // An instant at its start is in; one lasting across it too.
        assert!(overlaps(&event(at(15, 23), None), period));
        assert!(overlaps(&event(at(15, 22), Some(at(16, 1))), period));
        assert!(overlaps(&event(day(18), None), period));
    }

    #[test]
    fn a_length_past_an_instant_s_range_ends_nowhere_and_never_panics() {
        let huge = Event {
            start: Some(at(16, 8)),
            duration: chrono::Duration::try_days(99_999_999),
            ..Event::default()
        };
        assert_eq!(end_of(&huge), None);
        let period = (
            day_start(NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()),
            at_utc(17),
        );
        assert!(overlaps(&huge, period));
        assert_eq!(
            event(&Listed {
                agenda: None,
                event: huge
            })["fin"],
            Value::Null
        );
    }

    fn at_utc(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, day, 0, 0, 0).unwrap()
    }

    #[test]
    fn a_recurring_event_left_whole_is_kept_once_begun_and_said_so() {
        let weekly = Event {
            start: Some(at(2, 9)),
            end: Some(at(2, 10)),
            repeats: true,
            ..Event::default()
        };
        let period = (at_utc(16), at_utc(19));
        assert!(overlaps(&weekly, period));
        assert!(!overlaps(&weekly, (at_utc(1), at_utc(2))));
        let read = event(&Listed {
            agenda: None,
            event: weekly,
        });
        assert!(read["repetition"].as_str().unwrap().contains("répéter"));
    }

    #[test]
    fn an_event_reads_in_the_platform_s_zone() {
        let nine = Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap();
        let timed = Event {
            uid: Some("r1".into()),
            start: Some(When::At(nine)),
            duration: Some(chrono::Duration::minutes(90)),
            ..Event::default()
        };
        let listed = Listed {
            agenda: Some("Personnel".into()),
            event: timed,
        };
        let read = event(&listed);
        assert_eq!(read["debut"], "2026-11-16T09:00:00+01:00");
        assert_eq!(read["fin"], "2026-11-16T10:30:00+01:00");
        assert_eq!(read["objet"], Value::Null);
        assert_eq!(read["agenda"], "Personnel");
        let day = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let all_day = Event {
            start: Some(When::Day(day)),
            ..Event::default()
        };
        assert_eq!(end_of(&all_day), Some(When::Day(day.succ_opt().unwrap())));
        let within = (day_start(day), day_end(day));
        assert!(overlaps(&all_day, within));
        let before = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
        assert!(!overlaps(&all_day, (day_start(before), day_end(before))));
    }

    fn listed(event: Event) -> Value {
        super::event(&Listed {
            agenda: None,
            event,
        })
    }

    #[test]
    fn a_floating_time_reads_in_the_platform_s_zone_even_in_a_change_s_gap() {
        let floating = |text: &str| Event {
            start: Some(When::Floating(text.parse().unwrap())),
            ..Event::default()
        };
        assert_eq!(
            listed(floating("2026-11-16T09:00:00"))["debut"],
            "2026-11-16T09:00:00+01:00"
        );
        // Paris skips 02:00 to 03:00 that night: read with the offset
        // before (RFC 5545, 3.3.5), 01:30 UTC.
        assert_eq!(
            listed(floating("2027-03-28T02:30:00"))["debut"],
            "2027-03-28T03:30:00+02:00"
        );
    }

    #[test]
    fn an_end_that_does_not_follow_the_start_is_not_given() {
        for end in [at(16, 7), at(16, 8)] {
            let backwards = Event {
                start: Some(at(16, 8)),
                end: Some(end),
                ..Event::default()
            };
            assert_eq!(listed(backwards)["fin"], Value::Null);
        }
        let forwards = Event {
            start: Some(at(16, 8)),
            end: Some(at(16, 9)),
            ..Event::default()
        };
        assert_eq!(listed(forwards)["fin"], "2026-11-16T10:00:00+01:00");
    }

    #[test]
    fn attendees_past_those_shown_are_counted_even_past_those_kept() {
        let crowded = Event {
            attendees: vec![Person::default(); 50],
            attendee_count: 60,
            ..Event::default()
        };
        let read = listed(crowded);
        assert_eq!(
            read["participants"].as_array().unwrap().len(),
            MAX_ATTENDEES
        );
        assert_eq!(read["autresParticipants"], 50);
    }
}
