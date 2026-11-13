//! Tool `mail_calendar_events` (ADR-205): the events of the person the run
//! serves over a few days, in their agendas at the company's CalDAV server,
//! with the account they linked; each occurrence of a recurring one apart
//! where the server expands them, in the platform's zone, the earliest
//! first. Offered only where the company declared an agenda.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use mimorya_core::tools::{Citation, MailServerRole, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use validator::Validate;

use super::person::{MailReach, PersonAccount, refused, told};
use crate::delegated::shape::{MAX_SHORT_CHARS, bounded, day_end, day_start};
use crate::outbound::{CONNECTOR_USE, Cause};
use crate::standard_mail::CREDENTIALS_REFUSED;
use crate::standard_mail::caldav::{CalDav, Event, Person, When, agenda_events, agendas, in_zone};

/// The longest period read at once, in days.
pub const MAX_DAYS: i64 = 31;
/// The most events told.
const MAX_SHOWN: usize = 50;
const MAX_ATTENDEES: usize = 10;
/// What a company with no agenda declared is told.
const NO_CALDAV: &str = "La messagerie de l’entreprise n’a pas d’agenda CalDAV déclaré";

pub struct MailCalendarEvents {
    pub reach: MailReach,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MailCalendarEventsInput {
    /// Premier jour inclus (AAAA-MM-JJ)
    from: NaiveDate,
    /// Dernier jour inclus (AAAA-MM-JJ) : 31 jours au plus, premier et dernier compris
    to: NaiveDate,
}

/// Why the period is not read, if it is not.
fn period_problem(input: &MailCalendarEventsInput) -> Option<String> {
    let days = (input.to - input.from).num_days();
    if days < 0 {
        return Some("Le dernier jour précède le premier".into());
    }
    (days >= MAX_DAYS).then(|| format!("Période de {MAX_DAYS} jours au plus"))
}

/// An event read, with the agenda it is in.
struct Listed {
    agenda: Option<String>,
    event: Event,
}

#[async_trait]
impl TypedTool for MailCalendarEvents {
    type Input = MailCalendarEventsInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "mail_calendar_events",
            "Agenda (messagerie de l’entreprise)",
            "Renvoie les rendez-vous des agendas de la personne servie sur l’agenda CalDAV de l’entreprise, avec le compte qu’elle a relié, sur une période (31 jours au plus, bornes comprises) : objet, début et fin (heure de Paris), lieu, organisateur, participants.",
            ToolRisk::Read,
        )
        .requires([CONNECTOR_USE])
    }

    async fn run(
        &self,
        input: MailCalendarEventsInput,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        if let Some(problem) = period_problem(&input) {
            return Ok(ToolResult::error(problem));
        }
        let period = (day_start(input.from), day_end(input.to));
        let read = self.reach.within(self.read(ctx, period)).await;
        let mut listed = match read {
            Ok(listed) => listed,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        // The server's filter is not trusted: what falls outside is left.
        listed.retain(|listed| overlaps(&listed.event, period));
        listed.sort_by_key(|listed| listed.event.start.map(instant));
        let total = listed.len();
        let shown: Vec<&Listed> = listed.iter().take(MAX_SHOWN).collect();
        let mut content = json!({
            "periode": { "du": input.from, "au": input.to },
            "nombre": shown.len(),
            "rendezVous": shown.iter().map(|listed| event(listed)).collect::<Vec<_>>(),
        });
        if total > MAX_SHOWN {
            content["autres"] = json!(format!(
                "{} autre(s) rendez-vous : réduisez la période",
                total - MAX_SHOWN
            ));
        }
        Ok(ToolResult::ok(content)
            .with_summary(format!("{} rendez-vous", shown.len()))
            .with_citations(shown.iter().filter_map(|l| citation(&l.event)).collect()))
    }
}

impl MailCalendarEvents {
    /// The events of the run's person's agendas over `period`, or why
    /// there are none, in French.
    async fn read(
        &self,
        ctx: &ToolContext,
        period: (DateTime<Utc>, DateTime<Utc>),
    ) -> Result<Vec<Listed>, String> {
        let PersonAccount {
            account,
            config,
            login,
        } = self.reach.account(ctx).await?;
        let Some(address) = config.caldav.as_deref() else {
            return Err(NO_CALDAV.into());
        };
        let failed = |cause: Cause| async {
            match cause.summary == CREDENTIALS_REFUSED {
                true => refused(ctx, MailServerRole::CalDav, account.linked_at).await,
                false => told(&login, cause),
            }
        };
        let dav = match CalDav::new(address, &login, self.reach.network) {
            Ok(dav) => dav,
            Err(cause) => return Err(failed(cause).await),
        };
        let found = match agendas(&dav).await {
            Ok(found) => found,
            Err(cause) => return Err(failed(cause).await),
        };
        let mut listed = Vec::new();
        for agenda in &found {
            let events = match agenda_events(&dav, agenda, period).await {
                Ok(events) => events,
                Err(cause) => return Err(failed(cause).await),
            };
            listed.extend(events.into_iter().map(|event| Listed {
                agenda: agenda.name.clone(),
                event,
            }));
        }
        Ok(listed)
    }
}

/// An instant of a start or an end: a day from its start in the
/// platform's zone, a floating time read in it.
fn instant(when: When) -> DateTime<Utc> {
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
fn overlaps(event: &Event, (from, to): (DateTime<Utc>, DateTime<Utc>)) -> bool {
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

fn event(listed: &Listed) -> Value {
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
        "autresParticipants": event.attendees.len().saturating_sub(MAX_ATTENDEES),
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

fn citation(event: &Event) -> Option<Citation> {
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
    use chrono::TimeZone;

    use super::*;

    fn period(from: &str, to: &str) -> MailCalendarEventsInput {
        serde_json::from_value(json!({ "from": from, "to": to })).unwrap()
    }

    #[test]
    fn a_period_is_read_up_to_its_bound() {
        assert_eq!(period_problem(&period("2026-10-01", "2026-10-31")), None);
        assert!(period_problem(&period("2026-10-01", "2026-11-01")).is_some());
        assert!(period_problem(&period("2026-10-02", "2026-10-01")).is_some());
    }

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
}
