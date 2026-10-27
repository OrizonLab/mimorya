//! Tool `m365_calendar_event_create` (ADR-119): creates, in the calendar of
//! the person the run serves and with their own rights, the event they
//! validated: its subject, start (in the platform's zone) and length, its
//! place and text, and its attendees, whom Microsoft invites. Created once,
//! never repeated; always validated, unless the company's policy says
//! otherwise.

use async_trait::async_trait;
use chrono::{Duration, NaiveDateTime};
use mimorya_core::account_links::CALENDARS_WRITE;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use validator::Validate;

use super::compose::{addresses, excerpt, full_text, recipients, text_body, visible_line};
use super::graph::Graph;
use super::reach::write_as_person;
use super::shape::{bounded, in_platform_zone, short};
use crate::outbound::CONNECTOR_USE;

pub struct CalendarCreate {
    pub graph: Graph,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CalendarCreateInput {
    /// Objet, sur une ligne
    #[validate(length(min = 1, max = 255), custom(function = "visible_line"))]
    subject: String,
    /// Début, heure de Paris (AAAA-MM-JJTHH:MM:SS)
    start: NaiveDateTime,
    /// Durée en minutes : de 5 à 1 440 (une journée)
    #[validate(range(min = 5, max = 1440))]
    duration_minutes: u32,
    /// Lieu, sur une ligne
    #[serde(default)]
    #[validate(length(min = 1, max = 255), custom(function = "visible_line"))]
    location: Option<String>,
    /// Texte de l’invitation, en texte brut (retours à la ligne permis)
    #[serde(default)]
    #[validate(length(min = 1, max = 20000), custom(function = "full_text"))]
    body: Option<String>,
    /// Participants : adresses e-mail, 20 au plus ; Microsoft leur envoie l’invitation
    #[serde(default)]
    #[validate(length(max = 20), custom(function = "addresses"))]
    attendees: Vec<String>,
}

impl CalendarCreateInput {
    fn end(&self) -> NaiveDateTime {
        self.start + Duration::minutes(i64::from(self.duration_minutes))
    }
}

/// An instant of the platform's zone, as Graph reads it.
fn in_zone(at: NaiveDateTime) -> Value {
    json!({
        "dateTime": at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "timeZone": PLATFORM_TIME_ZONE.name(),
    })
}

/// The event Graph creates.
fn event(input: &CalendarCreateInput) -> Value {
    let mut event = json!({
        "subject": input.subject,
        "start": in_zone(input.start),
        "end": in_zone(input.end()),
        "attendees": recipients(&input.attendees, Some("required")),
    });
    if let Some(location) = &input.location {
        event["location"] = json!({ "displayName": location });
    }
    if let Some(body) = &input.body {
        event["body"] = text_body(body);
    }
    event
}

#[async_trait]
impl TypedTool for CalendarCreate {
    type Input = CalendarCreateInput;

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

    fn describe_action(&self, input: &CalendarCreateInput) -> Option<String> {
        let mut action = format!(
            "Créer le rendez-vous « {} » dans l’agenda Microsoft 365 relié, le {}, pendant {} min \
             (heure de Paris)",
            input.subject,
            input.start.format("%d/%m/%Y à %H:%M"),
            input.duration_minutes
        );
        if let Some(location) = &input.location {
            action.push_str(&format!(", lieu : {location}"));
        }
        if !input.attendees.is_empty() {
            action.push_str(&format!(
                ", invitation envoyée à {}",
                input.attendees.join(", ")
            ));
        }
        if let Some(body) = &input.body {
            action.push_str(&format!(" : « {} »", excerpt(body)));
        }
        Some(action)
    }

    async fn run(
        &self,
        input: CalendarCreateInput,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let url = self.graph.address(&["me", "events"], &[]);
        let zone = in_platform_zone();
        let body = event(&input);
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

    fn input(value: Value) -> CalendarCreateInput {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn an_event_is_written_in_the_platform_zone() {
        let created = event(&input(json!({
            "subject": "Point chantier",
            "start": "2026-10-28T23:30:00",
            "durationMinutes": 90,
            "attendees": ["paul@exemple.fr"],
        })));
        assert_eq!(created["start"]["dateTime"], "2026-10-28T23:30:00");
        assert_eq!(created["end"]["dateTime"], "2026-10-29T01:00:00");
        assert_eq!(created["end"]["timeZone"], "Europe/Paris");
        assert_eq!(
            created["attendees"][0]["emailAddress"]["address"],
            "paul@exemple.fr"
        );
        assert!(created.get("location").is_none() && created.get("body").is_none());
    }

    #[test]
    fn an_event_lasts_a_day_at_most() {
        let at = |minutes: u32| {
            input(json!({
                "subject": "Point",
                "start": "2026-10-28T10:00:00",
                "durationMinutes": minutes,
            }))
            .validate()
        };
        assert!(at(5).is_ok() && at(1440).is_ok());
        assert!(at(4).is_err() && at(1441).is_err());
    }
}
