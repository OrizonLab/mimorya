//! Tool `m365_calendar_events` (ADR-117): the events of the person the run
//! serves over a few days (theirs and those they are invited to), in the
//! platform's zone, the earliest first.

use async_trait::async_trait;
use chrono::NaiveDate;
use mimorya_core::account_links::CALENDARS_READ;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use validator::Validate;

use super::graph::Graph;
use super::reach::read_as_person;
use super::shape::{bounded, collection, day_end, day_start, graph_instant, in_platform_zone};
use super::shape::{people, person, short};
use crate::outbound::CONNECTOR_USE;

const SELECT: &str = "id,subject,start,end,isAllDay,isCancelled,location,organizer,attendees,\
     bodyPreview,webLink";
/// The longest period read at once, in days.
pub const MAX_DAYS: i64 = 31;
const MAX_EVENTS: usize = 50;
const MAX_ATTENDEES: usize = 10;

pub struct CalendarEvents {
    pub graph: Graph,
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

fn event(item: &Value) -> Value {
    // An attendee carries its `emailAddress` as a recipient does.
    let (attendees, more) = people(item.get("attendees"), MAX_ATTENDEES);
    json!({
        "id": item.get("id"),
        "objet": short(item, "/subject"),
        "debut": short(item, "/start/dateTime"),
        "fin": short(item, "/end/dateTime"),
        "journeeEntiere": item.get("isAllDay"),
        "annule": item.get("isCancelled"),
        "lieu": short(item, "/location/displayName"),
        "organisateur": item.get("organizer").map_or(Value::Null, person),
        "participants": attendees,
        "autresParticipants": more,
        "apercu": short(item, "/bodyPreview"),
        "lien": short(item, "/webLink"),
    })
}

fn citation(item: &Value) -> Option<Citation> {
    let id = item.get("id").and_then(Value::as_str)?;
    let subject = item
        .get("subject")
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
            "m365_calendar_events",
            "Agenda Microsoft 365",
            "Renvoie les rendez-vous de l’agenda Microsoft 365 de la personne servie sur une période (31 jours au plus, bornes comprises), avec ses propres droits : objet, début et fin (heure de Paris), lieu, organisateur, participants.",
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
        let url = self.graph.address(
            &["me", "calendarView"],
            &[
                ("startDateTime", graph_instant(day_start(input.from))),
                ("endDateTime", graph_instant(day_end(input.to))),
                ("$select", SELECT.to_string()),
                ("$orderby", "start/dateTime".into()),
                ("$top", MAX_EVENTS.to_string()),
            ],
        );
        let answer = match read_as_person(
            &self.graph,
            ctx,
            &[CALENDARS_READ],
            url,
            Some(&in_platform_zone()),
        )
        .await
        {
            Ok(answer) => answer,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        let (items, more) = collection(&answer);
        let mut content = json!({
            "periode": { "du": input.from, "au": input.to },
            "nombre": items.len(),
            "rendezVous": items.iter().map(event).collect::<Vec<_>>(),
        });
        if more {
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
}
