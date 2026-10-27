//! Tool `m365_calendar_event_create` (ADR-119): creates, in the calendar of
//! the person the run serves and with their own rights, the event they
//! validated: its subject, start (in the platform's zone) and length, its
//! place and text, and its attendees, whom Microsoft invites. Its start and
//! end are sent to Graph as instants, so that it lasts what was validated
//! even across a change of time. Created once, never repeated; always
//! validated, whatever the company's policies.

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveDateTime, TimeZone, Utc};
use mimorya_core::account_links::CALENDARS_WRITE;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use validator::{Validate, ValidationError};

use super::compose::{addresses, excerpt, full_text, recipients, text_body, visible_line};
use super::shape::in_platform_zone;
use crate::delegated::ProviderApi;
use crate::delegated::shape::{bounded, short};
use crate::delegated::write_as_person;
use crate::outbound::CONNECTOR_USE;

/// The years an event may start in.
const YEARS: std::ops::RangeInclusive<i32> = 2000..=2100;

pub struct CalendarCreate {
    pub graph: ProviderApi,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CalendarCreateInput {
    /// Objet, sur une ligne
    #[validate(length(min = 1, max = 255), custom(function = "visible_line"))]
    subject: String,
    /// Début, heure de Paris (AAAA-MM-JJTHH:MM:SS)
    #[validate(custom(function = "start_in_reach"))]
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

/// The instant a time of the platform's zone names: its first when the
/// clocks go back, none when they skip it.
fn instant(local: &NaiveDateTime) -> Option<DateTime<Utc>> {
    PLATFORM_TIME_ZONE
        .from_local_datetime(local)
        .earliest()
        .map(|at| at.with_timezone(&Utc))
}

/// A start within a century, at a time that exists in the platform's zone.
fn start_in_reach(start: &NaiveDateTime) -> Result<(), ValidationError> {
    let refused =
        |message: &'static str| Err(ValidationError::new("start").with_message(message.into()));
    if !YEARS.contains(&start.year()) {
        return refused("début entre l’an 2000 et l’an 2100");
    }
    if instant(start).is_none() {
        return refused("heure qui n’existe pas à Paris ce jour-là (passage à l’heure d’été)");
    }
    Ok(())
}

impl CalendarCreateInput {
    /// Its start and end, as instants: what was validated lasts as long.
    fn instants(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let start = instant(&self.start)?;
        let end = start.checked_add_signed(Duration::minutes(i64::from(self.duration_minutes)))?;
        Some((start, end))
    }
}

/// An instant as Graph reads it.
fn in_utc(at: DateTime<Utc>) -> Value {
    json!({
        "dateTime": at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "timeZone": "UTC",
    })
}

/// The event Graph creates, if its start is one (the validation saw to it).
fn event(input: &CalendarCreateInput) -> Option<Value> {
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

    /// When and to whom first, what the model wrote last: a subject cannot
    /// pass itself off as the attendees.
    fn describe_action(&self, input: &CalendarCreateInput) -> Option<String> {
        let mut action = format!(
            "Créer dans l’agenda Microsoft 365 relié un rendez-vous le {} (heure de Paris), \
             pendant {} min",
            input.start.format("%d/%m/%Y à %H:%M"),
            input.duration_minutes
        );
        match input.attendees.is_empty() {
            true => action.push_str(", sans participant"),
            false => action.push_str(&format!(
                ", invitation envoyée à {}",
                input.attendees.join(", ")
            )),
        }
        action.push_str(&format!(" — objet : « {} »", input.subject));
        if let Some(location) = &input.location {
            action.push_str(&format!(" ; lieu : « {location} »"));
        }
        if let Some(body) = &input.body {
            action.push_str(&format!(" ; texte : « {} »", excerpt(body)));
        }
        Some(action)
    }

    async fn run(
        &self,
        input: CalendarCreateInput,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
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

    fn input(start: &str, minutes: u32) -> CalendarCreateInput {
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

    #[test]
    fn an_event_lasts_a_day_at_most() {
        let at = |minutes: u32| input("2026-10-28T10:00:00", minutes).validate();
        assert!(at(5).is_ok() && at(1440).is_ok());
        assert!(at(4).is_err() && at(1441).is_err());
    }

    #[test]
    fn a_start_out_of_reach_is_refused_before_validation() {
        // Far off, its end would overflow; skipped when clocks go forward.
        for start in [
            "+262142-12-31T23:59:00",
            "1999-12-31T10:00:00",
            "2026-03-29T02:30:00",
        ] {
            let refused = input(start, 5).validate().unwrap_err();
            assert!(refused.field_errors().contains_key("start"), "{start}");
        }
        assert!(input("2100-12-31T22:00:00", 1440).validate().is_ok());
    }
}
