//! Scripted model steps for the planning scenario: propose a follow-up slot,
//! then ask to apply the proposal (which requires a human validation).

use std::sync::LazyLock;

use mimorya_agent::testing::{
    RecordedRequest, ScriptedReply, ScriptedStep, call, last_tool_results,
};
use mimorya_core::demo::user;
use regex::Regex;
use serde_json::json;

use super::next_monday;

static PROPOSAL_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""propositionId": "([0-9a-f-]+)""#).unwrap());

/// Content of the first tool result shown to the model in this request.
pub fn first_result(request: &RecordedRequest) -> String {
    last_tool_results(request)
        .first()
        .map(|r| r.content.clone())
        .unwrap_or_default()
}

pub fn propose(intervention_id: &str, start_time: &str) -> ScriptedStep {
    ScriptedStep::calls(
        "",
        vec![call(
            "planning_propose",
            json!({
                "title": format!("Suivi {intervention_id}"),
                "items": [{
                    "interventionId": intervention_id,
                    "technicianId": user::LEA,
                    "date": next_monday(),
                    "startTime": start_time,
                    "durationHours": 1,
                    "reason": "Passage à reprogrammer",
                }],
            }),
        )],
    )
}

/// Applies the proposal created by the previous step.
pub fn apply_proposal(text: &str) -> ScriptedStep {
    apply_proposal_times(text, 1)
}

/// Asks `times` in one step to apply the proposal the previous step created:
/// as many validation requests.
pub fn apply_proposal_times(text: &str, times: usize) -> ScriptedStep {
    let text = text.to_string();
    ScriptedStep::dynamic(move |request| apply_reply(request, &text, times))
}

/// The answer asking `times` to apply the proposal `request` shows.
pub fn apply_reply(request: &RecordedRequest, text: &str, times: usize) -> ScriptedReply {
    let content = first_result(request);
    let proposal_id = PROPOSAL_ID
        .captures(&content)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| panic!("no proposal in {content}"));
    let apply = call("planning_apply", json!({ "proposalId": proposal_id }));
    ScriptedReply::calls(text, vec![apply; times])
}
