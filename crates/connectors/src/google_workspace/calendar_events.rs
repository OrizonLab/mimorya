//! Tool `google_calendar_events` (ADR-122): the events of the person the
//! run serves over a few days, in their primary calendar (theirs and those
//! they are invited to), each occurrence of a recurring one apart, in the
//! platform's zone, the earliest first.

use async_trait::async_trait;
use chrono::NaiveDate;
use mimorya_core::account_links::CALENDAR_READONLY;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use validator::Validate;

use super::text::html_to_text;
use crate::delegated::shape::{MAX_SHORT_CHARS, bounded, day_end, day_start, short, utc_instant};
use crate::delegated::{ProviderApi, read_as_person};
use crate::outbound::CONNECTOR_USE;

/// The longest period read at once, in days.
pub const MAX_DAYS: i64 = 31;
const MAX_EVENTS: usize = 50;
const MAX_ATTENDEES: usize = 10;

pub struct CalendarEvents {
    pub calendar: ProviderApi,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct CalendarEventsInput {
    /// Premier jour inclus (AAAA-MM-JJ)
    from: NaiveDate,
    /// Dernier jour inclus (AAAA-MM-JJ) : 31 jours au plus, premier et dernier compris
    to: NaiveDate,
}

/// Why the period is not read, if it is not.
fn period_problem(input: &CalendarEventsInput) -> Option<String> {
    let days = (input.to - input.from).num_days();
    if days < 0 {
        return Some("Le dernier jour précède le premier".into());
    }
    (days >= MAX_DAYS).then(|| format!("Période de {MAX_DAYS} jours au plus"))
}

/// "Nom <adresse>" of a person of Google Calendar (`{ displayName, email }`).
fn person(who: &Value) -> Value {
    let name = short(who, "/displayName");
    let address = short(who, "/email");
    match (name.as_str(), address.as_str()) {
        (Some(name), Some(address)) if name != address => json!(format!("{name} <{address}>")),
        (_, Some(address)) => json!(address),
        (Some(name), None) => json!(name),
        (None, None) => Value::Null,
    }
}

/// The start or end of an event: its instant, or its day when it lasts
/// whole days.
fn bound(item: &Value, which: &str) -> Value {
    let at = short(item, &format!("/{which}/dateTime"));
    if at.is_null() {
        short(item, &format!("/{which}/date"))
    } else {
        at
    }
}

fn event(item: &Value) -> Value {
    let attendees = item
        .get("attendees")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let description = item
        .get("description")
        .and_then(Value::as_str)
        .map_or(Value::Null, |text| {
            json!(bounded(&html_to_text(text), MAX_SHORT_CHARS).0)
        });
    json!({
        "id": item.get("id"),
        "objet": short(item, "/summary"),
        "debut": bound(item, "start"),
        "fin": bound(item, "end"),
        "journeeEntiere": item.pointer("/start/date").is_some(),
        "annule": item.get("status").and_then(Value::as_str) == Some("cancelled"),
        "lieu": short(item, "/location"),
        "organisateur": item.get("organizer").map_or(Value::Null, person),
        "participants": attendees.iter().take(MAX_ATTENDEES).map(person).collect::<Vec<_>>(),
        "autresParticipants": attendees.len().saturating_sub(MAX_ATTENDEES),
        "apercu": description,
        "lien": short(item, "/htmlLink"),
    })
}

fn citation(item: &Value) -> Option<Citation> {
    let id = item.get("id").and_then(Value::as_str)?;
    let subject = item
        .get("summary")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("(sans objet)");
    Some(Citation::new("event", id).labelled(bounded(subject, 120).0))
}

#[async_trait]
impl TypedTool for CalendarEvents {
    type Input = CalendarEventsInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "google_calendar_events",
            "Agenda Google",
            "Renvoie les rendez-vous de l’agenda Google principal de la personne servie sur une période (31 jours au plus, bornes comprises), avec ses propres droits : objet, début et fin (heure de Paris), lieu, organisateur, participants.",
            ToolRisk::Read,
        )
        .requires([CONNECTOR_USE])
    }

    async fn run(
        &self,
        input: CalendarEventsInput,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        if let Some(problem) = period_problem(&input) {
            return Ok(ToolResult::error(problem));
        }
        let url = self.calendar.address(
            &["calendars", "primary", "events"],
            &[
                ("timeMin", utc_instant(day_start(input.from))),
                ("timeMax", utc_instant(day_end(input.to))),
                ("singleEvents", "true".into()),
                ("orderBy", "startTime".into()),
                ("maxResults", MAX_EVENTS.to_string()),
                ("timeZone", PLATFORM_TIME_ZONE.name().into()),
            ],
        );
        let answer =
            match read_as_person(&self.calendar, ctx, &[CALENDAR_READONLY], url, None).await {
                Ok(answer) => answer,
                Err(why) => return Ok(ToolResult::error(why)),
            };
        let items = answer
            .get("items")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let mut content = json!({
            "periode": { "du": input.from, "au": input.to },
            "nombre": items.len(),
            "rendezVous": items.iter().map(event).collect::<Vec<_>>(),
        });
        if answer.get("nextPageToken").is_some() {
            content["autres"] = json!(format!(
                "Plus de {MAX_EVENTS} rendez-vous : réduisez la période"
            ));
        }
        Ok(ToolResult::ok(content)
            .with_summary(format!("{} rendez-vous", items.len()))
            .with_citations(items.iter().filter_map(citation).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn period(from: &str, to: &str) -> CalendarEventsInput {
        serde_json::from_value(json!({ "from": from, "to": to })).unwrap()
    }

    #[test]
    fn a_period_is_read_up_to_its_bound() {
        assert_eq!(period_problem(&period("2026-10-01", "2026-10-01")), None);
        assert_eq!(period_problem(&period("2026-10-01", "2026-10-31")), None);
        assert!(period_problem(&period("2026-10-01", "2026-11-01")).is_some());
        assert!(period_problem(&period("2026-10-02", "2026-10-01")).is_some());
    }

    #[test]
    fn an_event_reads_in_a_few_fields() {
        let all_day = json!({
            "id": "e1",
            "summary": "Séminaire",
            "start": { "date": "2026-10-28" },
            "end": { "date": "2026-10-29" },
            "status": "cancelled",
            "organizer": { "email": "lea@neo.example", "displayName": "Léa" },
            "attendees": [{ "email": "paul@neo.example" }],
            "description": "<p>Ordre&nbsp;du jour</p>",
        });
        let read = event(&all_day);
        assert_eq!(read["debut"], "2026-10-28");
        assert_eq!(read["journeeEntiere"], true);
        assert_eq!(read["annule"], true);
        assert_eq!(read["organisateur"], "Léa <lea@neo.example>");
        assert_eq!(read["participants"], json!(["paul@neo.example"]));
        assert_eq!(read["apercu"], "Ordre du jour");
        let timed = json!({ "id": "e2", "start": { "dateTime": "2026-10-28T09:00:00+01:00" } });
        assert_eq!(event(&timed)["debut"], "2026-10-28T09:00:00+01:00");
        assert_eq!(event(&timed)["journeeEntiere"], false);
        assert_eq!(
            citation(&timed).unwrap().label.as_deref(),
            Some("(sans objet)")
        );
    }
}
