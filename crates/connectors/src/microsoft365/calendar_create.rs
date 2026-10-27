//! Tool `m365_calendar_event_create` (ADR-119): creates, in the calendar of
//! the person the run serves and with their own rights, the event they
//! validated: its subject, start (in the platform's zone) and length, its
//! place and text, and its attendees, whom Microsoft invites. Its start and
//! end are sent to Graph as instants, so that it lasts what was validated
//! even across a change of time. Created once, never repeated; always
//! validated, whatever the company's policies.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mimorya_core::account_links::CALENDARS_WRITE;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use serde_json::{Value, json};

use super::shape::{in_platform_zone, recipients, text_body};
use crate::delegated::shape::{bounded, short};
use crate::delegated::{NewEvent, ProviderApi, write_as_person};
use crate::outbound::CONNECTOR_USE;

pub struct CalendarCreate {
    pub graph: ProviderApi,
}

/// An instant as Graph reads it.
fn in_utc(at: DateTime<Utc>) -> Value {
    json!({
        "dateTime": at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "timeZone": "UTC",
    })
}

/// The event Graph creates, if its start is one (the validation saw to it).
fn event(input: &NewEvent) -> Option<Value> {
    let (start, end) = input.instants()?;
    let mut event = json!({
        "subject": input.subject,
        "start": in_utc(start),
        "end": in_utc(end),
        "attendees": recipients(&input.attendees, Some("required")),
    });
    if let Some(location) = &input.location {
        event["location"] = json!({ "displayName": location });
    }
    if let Some(body) = &input.body {
        event["body"] = text_body(body);
    }
    Some(event)
}

#[async_trait]
impl TypedTool for CalendarCreate {
    type Input = NewEvent;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "m365_calendar_event_create",
            "Rendez-vous Microsoft 365",
            "Crée un rendez-vous dans l’agenda Microsoft 365 de la personne servie, avec ses propres droits : objet, début (heure de Paris), durée, lieu, texte et participants, exactement tels qu’ils sont validés ; Microsoft envoie l’invitation aux participants.",
            ToolRisk::External,
        )
        .requires([CONNECTOR_USE])
        .always_requires_approval()
    }

    fn describe_action(&self, input: &NewEvent) -> Option<String> {
        Some(input.described(self.graph.kind().product()))
    }

    async fn run(&self, input: NewEvent, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let Some(body) = event(&input) else {
            return Ok(ToolResult::error(
                "Début du rendez-vous hors des dates permises",
            ));
        };
        let url = self.graph.address(&["me", "events"], &[]);
        let zone = in_platform_zone();
        let created = match write_as_person(
            &self.graph,
            ctx,
            &[CALENDARS_WRITE],
            url,
            &body,
            Some(&zone),
        )
        .await
        {
            Ok(created) => created.unwrap_or_default(),
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let content = json!({
            "cree": true,
            "id": created.get("id"),
            "objet": input.subject,
            "debut": short(&created, "/start/dateTime"),
            "fin": short(&created, "/end/dateTime"),
            "participants": input.attendees,
            "lien": short(&created, "/webLink"),
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

    fn input(start: &str, minutes: u32) -> NewEvent {
        serde_json::from_value(json!({
            "subject": "Point chantier",
            "start": start,
            "durationMinutes": minutes,
            "attendees": ["paul@exemple.fr"],
        }))
        .unwrap()
    }

    #[test]
    fn an_event_starts_and_ends_at_the_instants_validated() {
        // Paris in winter: an hour behind UTC.
        let created = event(&input("2026-11-28T23:30:00", 90)).unwrap();
        assert_eq!(created["start"]["dateTime"], "2026-11-28T22:30:00");
        assert_eq!(created["end"]["dateTime"], "2026-11-29T00:00:00");
        assert_eq!(created["end"]["timeZone"], "UTC");
        assert_eq!(
            created["attendees"][0]["emailAddress"]["address"],
            "paul@exemple.fr"
        );
        assert!(created.get("location").is_none() && created.get("body").is_none());
        // The night clocks go back: 02:30 is first met in summer time, and an
        // hour later is still an hour later.
        let back = event(&input("2026-10-25T02:30:00", 60)).unwrap();
        assert_eq!(back["start"]["dateTime"], "2026-10-25T00:30:00");
        assert_eq!(back["end"]["dateTime"], "2026-10-25T01:30:00");
    }
}
