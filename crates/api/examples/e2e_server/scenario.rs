//! The reference scenario (ROADMAP, A11/A13), as the scripted model plays
//! it: read this week's intervention reports, propose a follow-up for the
//! unfinished one, ask for validation to apply it, then sum up. Each run
//! plays it from its own history, whatever ran before it in this start, so
//! that the end-to-end tests pass in any shard of the suite (ADR-199): its
//! next step follows from the last tool it called, and its follow-up is
//! planned next Monday at 13 h, unless a run of this start had its planning
//! applied there (a slot a validated run took would make the proposal fail).

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{Days, Utc};
use mimorya_agent::llm::{
    LlmError, LlmErrorKind, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolResult,
};
use mimorya_agent::testing::{ScriptedCall, ScriptedProvider, ScriptedReply, ScriptedStep};
use mimorya_connectors::demo_interventions::dates::{iso_date, monday_of};
use mimorya_core::demo::user;
use regex::Regex;
use serde_json::{Value, json};

/// The title of the scenario's last answer, which the tests wait for.
const SUMMARY_TITLE: &str = "Synthèse des interventions";

/// The hours a follow-up may start at, in the order they are offered: two
/// hours each, none overlapping another, all free in the demo planning.
/// Only `reference-scenario.spec.ts` validates its run, so it always gets
/// the first; a run after it in the same start gets the next.
const HOURS: [u32; 4] = [13, 9, 15, 11];
/// How long a follow-up lasts, in hours.
const DURATION_HOURS: u32 = 2;

/// The model that plays the reference scenario for every conversation that
/// reaches it, and remembers the hours where a planning was applied.
pub struct Reference {
    applied: Mutex<Vec<u32>>,
}

impl Reference {
    pub fn new() -> Self {
        Self {
            applied: Mutex::new(Vec::new()),
        }
    }

    /// The first hour whose slot overlaps no applied one.
    fn free_hour(&self) -> u32 {
        let applied = self.applied.lock().expect("applied hours");
        HOURS
            .into_iter()
            .find(|hour| {
                applied
                    .iter()
                    .all(|taken| hour.abs_diff(*taken) >= DURATION_HOURS)
            })
            .unwrap_or(HOURS[0])
    }

    /// The run's next step, from the last tool it called.
    fn step(&self, request: &LlmRequest) -> Result<ScriptedReply, LlmError> {
        let Some(called) = last_call(&request.messages) else {
            return Ok(search());
        };
        let results = last_results(&request.messages);
        match called.as_str() {
            "interventions_search" => Ok(propose(self.free_hour())),
            "planning_propose" => Ok(apply(&results)),
            "planning_apply" => {
                let hour = proposed_hour(&request.messages).unwrap_or(HOURS[0]);
                let error = results.into_iter().find(|r| r.is_error).map(|r| r.content);
                if error.is_none() {
                    self.applied.lock().expect("applied hours").push(hour);
                }
                Ok(summary(hour, error))
            }
            other => Err(LlmError {
                provider: "scripted".into(),
                kind: LlmErrorKind::Unknown,
                message: format!("Scénario de référence : aucune étape après « {other} »."),
            }),
        }
    }
}

#[async_trait]
impl LlmProvider for Reference {
    fn id(&self) -> &str {
        "scripted"
    }

    fn model(&self) -> &str {
        "scripted-model"
    }

    /// Answered as the scripted model answers one step: its text streamed,
    /// its usage, its name.
    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let reply = self.step(request)?;
        ScriptedProvider::new(vec![ScriptedStep::Fixed(reply)])
            .complete(request)
            .await
    }
}

/// The tool the run called last, when the request ends with its results.
fn last_call(messages: &[LlmMessage]) -> Option<String> {
    if !matches!(messages.last(), Some(LlmMessage::ToolResults { .. })) {
        return None;
    }
    messages.iter().rev().find_map(|message| match message {
        LlmMessage::Assistant { tool_calls, .. } => tool_calls.first().map(|c| c.name.clone()),
        _ => None,
    })
}

/// The results the request ends with.
fn last_results(messages: &[LlmMessage]) -> Vec<LlmToolResult> {
    match messages.last() {
        Some(LlmMessage::ToolResults { results }) => results.clone(),
        _ => Vec::new(),
    }
}

/// The hour the run's last proposal starts at.
fn proposed_hour(messages: &[LlmMessage]) -> Option<u32> {
    messages.iter().rev().find_map(|message| match message {
        LlmMessage::Assistant { tool_calls, .. } => tool_calls
            .iter()
            .find(|c| c.name == "planning_propose")
            .and_then(|c| c.input["items"][0]["startTime"].as_str())
            .and_then(|time| time.split(':').next())
            .and_then(|hour| hour.parse().ok()),
        _ => None,
    })
}

/// A call of `name`, under an identifier of its own within the run.
fn call(name: &str, input: Value) -> ScriptedCall {
    ScriptedCall {
        name: name.into(),
        input,
        id: Some(format!("reference-{}", uuid::Uuid::new_v4())),
    }
}

fn search() -> ScriptedReply {
    let monday = monday_of(Utc::now().date_naive());
    let week = json!({ "from": iso_date(monday), "to": iso_date(monday + Days::new(4)) });
    ScriptedReply::calls(
        "Je consulte les comptes rendus de la semaine.",
        vec![call("interventions_search", week)],
    )
}

fn propose(hour: u32) -> ScriptedReply {
    let next_monday = iso_date(monday_of(Utc::now().date_naive()) + Days::new(7));
    ScriptedReply::calls(
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
                    "durationHours": DURATION_HOURS,
                    "reason": "Terminer la mise en service",
                }],
            }),
        )],
    )
}

/// The proposal's identifier comes from the previous tool result.
fn apply(results: &[LlmToolResult]) -> ScriptedReply {
    let content = results
        .first()
        .map(|r| r.content.as_str())
        .unwrap_or_default();
    let proposal = Regex::new(r#""propositionId": "([0-9a-f-]+)""#)
        .ok()
        .and_then(|pattern| pattern.captures(content).map(|c| c[1].to_string()))
        .unwrap_or_default();
    ScriptedReply::calls(
        "Je demande votre validation pour appliquer le planning.",
        vec![call("planning_apply", json!({ "proposalId": proposal }))],
    )
}

/// The summary says what became of the validation: applied, refused (by
/// whom the agent acts for or their manager), or failed.
fn summary(hour: u32, error: Option<String>) -> ScriptedReply {
    ScriptedReply::text(match error {
        Some(content) if content.contains("Action refusée lors de la validation") => format!(
            "## {SUMMARY_TITLE}\n\n- **INT-2049** : intervention non terminée ; le suivi \
             proposé lundi prochain à {hour} h avec Léa n'est pas appliqué.\n\n\
             Le planning est inchangé : la validation a été refusée."
        ),
        Some(content) => format!(
            "## {SUMMARY_TITLE}\n\n- **INT-2049** : le suivi proposé lundi prochain à \
             {hour} h avec Léa n'a pas pu être appliqué : {content}"
        ),
        None => format!(
            "## {SUMMARY_TITLE}\n\n- **INT-2049** : intervention non terminée, suivi \
             programmé lundi prochain à {hour} h avec Léa.\n\nLe planning de suivi est \
             appliqué."
        ),
    })
}
