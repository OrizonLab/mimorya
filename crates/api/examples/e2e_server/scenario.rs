//! The reference scenario (ROADMAP, A11/A13), as the scripted model plays
//! it: read this week's intervention reports, propose a follow-up for the
//! unfinished one, ask for validation to apply it, then sum up.

use chrono::{Days, Utc};
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_connectors::demo_interventions::dates::{iso_date, monday_of};
use mimorya_core::demo::user;
use regex::Regex;
use serde_json::json;

/// The title of the scenario's last answer, which the tests wait for.
const SUMMARY_TITLE: &str = "Synthèse des interventions";

/// The scenario, its follow-up planned next Monday at `hour` (each run its
/// own slot: a slot already taken would make the proposal fail).
pub fn reference(hour: u32) -> Vec<ScriptedStep> {
    let monday = monday_of(Utc::now().date_naive());
    let week = json!({ "from": iso_date(monday), "to": iso_date(monday + Days::new(4)) });
    let next_monday = iso_date(monday + Days::new(7));
    vec![
        ScriptedStep::calls(
            "Je consulte les comptes rendus de la semaine.",
            vec![call("interventions_search", week)],
        ),
        ScriptedStep::calls(
            "",
            vec![call(
                "planning_propose",
                json!({
                    "title": "Suivi des interventions non terminées",
                    "items": [{
                        "interventionId": "INT-2049",
                        "technicianId": user::LEA,
                        "date": next_monday,
                        "startTime": format!("{hour:02}:00"),
                        "durationHours": 2,
                        "reason": "Terminer la mise en service",
                    }],
                }),
            )],
        ),
        // The proposal's identifier comes from the previous tool result.
        ScriptedStep::dynamic(|request| {
            let content = last_tool_results(request)
                .first()
                .map(|result| result.content.clone())
                .unwrap_or_default();
            let proposal = Regex::new(r#""propositionId": "([0-9a-f-]+)""#)
                .ok()
                .and_then(|pattern| pattern.captures(&content).map(|c| c[1].to_string()))
                .unwrap_or_default();
            ScriptedReply::calls(
                "Je demande votre validation pour appliquer le planning.",
                vec![call("planning_apply", json!({ "proposalId": proposal }))],
            )
        }),
        ScriptedStep::text(format!(
            "## {SUMMARY_TITLE}\n\n- **INT-2049** : intervention non terminée, suivi programmé \
             lundi prochain à {hour} h avec Léa.\n\nLe planning de suivi est appliqué."
        )),
    ]
}
