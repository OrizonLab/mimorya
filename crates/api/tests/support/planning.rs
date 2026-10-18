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
    let text = text.to_string();
    ScriptedStep::dynamic(move |request| {
        let content = first_result(request);
        let proposal_id = PROPOSAL_ID
            .captures(&content)
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| panic!("no proposal in {content}"));
        ScriptedReply::calls(
            text.clone(),
            vec![call("planning_apply", json!({ "proposalId": proposal_id }))],
        )
    })
}
