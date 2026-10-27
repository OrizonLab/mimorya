//! The event a tool creates in a person's calendar, whichever provider
//! creates it (ADR-119, ADR-123), as the model asks it and the person
//! validates it: its subject, its start in the platform's zone and its
//! length, its place, text and attendees, each hiding nothing (`compose`);
//! its start and end as instants, so that it lasts what was validated even
//! across a change of time; and its description, when and whom first.

use chrono::{DateTime, Datelike, Duration, NaiveDateTime, TimeZone, Utc};
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use super::compose::{addresses, excerpt, full_text, visible_line};

/// The years an event may start in.
const YEARS: std::ops::RangeInclusive<i32> = 2000..=2100;

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NewEvent {
    /// Objet, sur une ligne
    #[validate(length(min = 1, max = 255), custom(function = "visible_line"))]
    pub subject: String,
    /// Début, heure de Paris (AAAA-MM-JJTHH:MM:SS)
    #[validate(custom(function = "start_in_reach"))]
    pub start: NaiveDateTime,
    /// Durée en minutes : de 5 à 1 440 (une journée)
    #[validate(range(min = 5, max = 1440))]
    pub duration_minutes: u32,
    /// Lieu, sur une ligne
    #[serde(default)]
    #[validate(length(min = 1, max = 255), custom(function = "visible_line"))]
    pub location: Option<String>,
    /// Texte de l’invitation, en texte brut (retours à la ligne permis)
    #[serde(default)]
    #[validate(length(min = 1, max = 20000), custom(function = "full_text"))]
    pub body: Option<String>,
    /// Participants : adresses e-mail, 20 au plus ; chacun reçoit l’invitation
    #[serde(default)]
    #[validate(length(max = 20), custom(function = "addresses"))]
    pub attendees: Vec<String>,
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

impl NewEvent {
    /// Its start and end, as instants: what was validated lasts as long.
    pub fn instants(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let start = instant(&self.start)?;
        let end = start.checked_add_signed(Duration::minutes(i64::from(self.duration_minutes)))?;
        Some((start, end))
    }

    /// When and to whom first, what the model wrote last: a subject cannot
    /// pass itself off as the attendees.
    pub fn described(&self, product: &str) -> String {
        let mut action = format!(
            "Créer dans l’agenda {product} relié un rendez-vous le {} (heure de Paris), \
             pendant {} min",
            self.start.format("%d/%m/%Y à %H:%M"),
            self.duration_minutes
        );
        match self.attendees.is_empty() {
            true => action.push_str(", sans participant"),
            false => action.push_str(&format!(
                ", invitation envoyée à {}",
                self.attendees.join(", ")
            )),
        }
        action.push_str(&format!(" — objet : « {} »", self.subject));
        if let Some(location) = &self.location {
            action.push_str(&format!(" ; lieu : « {location} »"));
        }
        if let Some(body) = &self.body {
            action.push_str(&format!(" ; texte : « {} »", excerpt(body)));
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn event(start: &str, minutes: u32) -> NewEvent {
        serde_json::from_value(json!({
            "subject": "Point chantier",
            "start": start,
            "durationMinutes": minutes,
            "attendees": ["paul@exemple.fr"],
        }))
        .unwrap()
    }

    #[test]
    fn an_event_lasts_what_was_validated() {
        // Paris in winter: an hour behind UTC.
        let (start, end) = event("2026-11-28T23:30:00", 90).instants().unwrap();
        assert_eq!(start.to_rfc3339(), "2026-11-28T22:30:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-11-29T00:00:00+00:00");
        // The night clocks go back: 02:30 is first met in summer time, and an
        // hour later is still an hour later.
        let (start, end) = event("2026-10-25T02:30:00", 60).instants().unwrap();
        assert_eq!(start.to_rfc3339(), "2026-10-25T00:30:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-10-25T01:30:00+00:00");
    }

    #[test]
    fn an_event_lasts_a_day_at_most() {
        let at = |minutes: u32| event("2026-10-28T10:00:00", minutes).validate();
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
            let refused = event(start, 5).validate().unwrap_err();
            assert!(refused.field_errors().contains_key("start"), "{start}");
        }
        assert!(event("2100-12-31T22:00:00", 1440).validate().is_ok());
    }

    #[test]
    fn its_description_says_when_and_to_whom_first() {
        let described = event("2026-11-03T09:30:00", 45).described("Google Workspace");
        assert!(
            described.starts_with(
                "Créer dans l’agenda Google Workspace relié un rendez-vous le 03/11/2026 à \
                 09:30 (heure de Paris), pendant 45 min, invitation envoyée à paul@exemple.fr \
                 — objet : « Point chantier »"
            ),
            "{described}"
        );
    }
}
