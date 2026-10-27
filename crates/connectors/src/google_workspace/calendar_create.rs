//! Tool `google_calendar_event_create` (ADR-123): creates, in the primary
//! calendar of the person the run serves and with their own rights, the
//! event they validated; Google e-mails its invitation to each attendee
//! (`sendUpdates=all`). Its start and end are sent as instants, in UTC,
//! with the platform's zone for how Google shows them: it lasts what was
//! validated, even across a change of time. Created once, never repeated;
//! always validated, whatever the company's policies.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mimorya_core::account_links::CALENDAR_EVENTS;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use serde_json::{Value, json};

use crate::delegated::shape::{bounded, short, utc_instant};
use crate::delegated::{NewEvent, ProviderApi, write_as_person};
use crate::outbound::CONNECTOR_USE;

pub struct CalendarCreate {
    pub calendar: ProviderApi,
}

/// An instant as Google Calendar reads it.
fn at(instant: DateTime<Utc>) -> Value {
    json!({
        "dateTime": utc_instant(instant),
        "timeZone": PLATFORM_TIME_ZONE.name(),
    })
}

/// The event Google creates, if its start is one (the validation saw to
/// it).
fn event(input: &NewEvent) -> Option<Value> {
    let (start, end) = input.instants()?;
    let attendees: Vec<Value> = input
        .attendees
        .iter()
        .map(|address| json!({ "email": address }))
        .collect();
    let mut event = json!({
        "summary": input.subject,
        "start": at(start),
        "end": at(end),
        "attendees": attendees,
    });
    if let Some(location) = &input.location {
        event["location"] = json!(location);
    }
    if let Some(body) = &input.body {
        event["description"] = json!(body);
    }
    Some(event)
}

#[async_trait]
impl TypedTool for CalendarCreate {
    type Input = NewEvent;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "google_calendar_event_create",
            "Rendez-vous Google Agenda",
            "Crée un rendez-vous dans l’agenda Google de la personne servie, avec ses propres droits : objet, début (heure de Paris), durée, lieu, texte et participants, exactement tels qu’ils sont validés ; Google envoie l’invitation aux participants.",
            ToolRisk::External,
        )
        .requires([CONNECTOR_USE])
        .always_requires_approval()
    }

    fn describe_action(&self, input: &NewEvent) -> Option<String> {
        Some(input.described(self.calendar.kind().product()))
    }

    async fn run(&self, input: NewEvent, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let Some(body) = event(&input) else {
            return Ok(ToolResult::error(
                "Début du rendez-vous hors des dates permises",
            ));
        };
        let url = self.calendar.address(
            &["calendars", "primary", "events"],
            &[("sendUpdates", "all".into())],
        );
        let created = match write_as_person(
            &self.calendar,
            ctx,
            &[CALENDAR_EVENTS],
            url,
            &body,
            None,
        )
        .await
        {
            Ok(created) => created.unwrap_or_default(),
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let content = json!({
            "cree": true,
            "id": short(&created, "/id"),
            "objet": input.subject,
            "debut": short(&created, "/start/dateTime"),
            "fin": short(&created, "/end/dateTime"),
            "participants": input.attendees,
            "lien": short(&created, "/htmlLink"),
        });
        let citations = created
            .get("id")
            .and_then(Value::as_str)
            .map(|id| Citation::new("event", id).labelled(bounded(&input.subject, 120).0))
            .into_iter()
            .collect();
        Ok(ToolResult::ok(content)
            .with_summary("Rendez-vous créé")
            .with_citations(citations))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_is_sent_as_instants_shown_in_the_platforms_zone() {
        let input: NewEvent = serde_json::from_value(json!({
            "subject": "Point chantier",
            "start": "2026-11-28T23:30:00",
            "durationMinutes": 90,
            "attendees": ["paul@exemple.fr"],
        }))
        .unwrap();
        let created = event(&input).unwrap();
        assert_eq!(
            created,
            json!({
                "summary": "Point chantier",
                "start": { "dateTime": "2026-11-28T22:30:00Z", "timeZone": "Europe/Paris" },
                "end": { "dateTime": "2026-11-29T00:00:00Z", "timeZone": "Europe/Paris" },
                "attendees": [{ "email": "paul@exemple.fr" }],
            })
        );
    }
}
