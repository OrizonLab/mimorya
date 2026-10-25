//! Reference scenario (docs/AGENT_ENGINE.md §10), end to end with a scripted
//! model: the engine, the policy enforcement point, the demo connector, human
//! validation, verification and memory capitalisation work together.

mod support;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mimorya_agent::runtime::{ApprovalDecision, RunOutcome};
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_core::demo::{unit, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemoryStatus};
use serde_json::{Value, json};
use support::{Scenario, WEEK, first_user_turn, marc_plan, parse_result, tool_names};

fn search_week() -> Value {
    json!({ "from": WEEK.0, "to": WEEK.1 })
}

#[tokio::test]
async fn runs_the_full_mission_for_the_technical_manager_with_validation_and_capitalisation() {
    let proposal_id = Arc::new(Mutex::new(String::new()));
    let captured = proposal_id.clone();
    let steps = vec![
        // 1. Permissions are reflected in the tools offered; the agent loads the skill and searches.
        ScriptedStep::dynamic(|req| {
            assert_eq!(
                tool_names(req),
                [
                    "document_read",
                    "document_search",
                    "interventions_get",
                    "interventions_search",
                    "memory_remember",
                    "memory_search",
                    "planning_apply",
                    "planning_propose",
                    "skill_read",
                    "team_availability",
                ]
            );
            assert!(req.system.contains("synthese-hebdo-interventions"));
            let turn = first_user_turn(req);
            assert!(turn.contains("Clôture d’une intervention"));
            // Personal memory of another user never reaches the context.
            assert!(!turn.contains("Lucas souhaite"));
            ScriptedReply::calls(
                "Je charge la procédure et je recherche les interventions de la semaine.",
                vec![
                    call(
                        "skill_read",
                        json!({ "name": "synthese-hebdo-interventions" }),
                    ),
                    call("interventions_search", search_week()),
                ],
            )
        }),
        // 2-4. Data retrieval scoped to the department, details of the anomalies.
        ScriptedStep::dynamic(|req| {
            let data = parse_result(&last_tool_results(req)[1].content);
            assert_eq!(data["nombre"], 13);
            let int_2043 = data["interventions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["id"] == "INT-2043")
                .unwrap();
            assert_eq!(int_2043["anomalies"].as_array().unwrap().len(), 1);
            ScriptedReply::calls(
                "",
                ["INT-2043", "INT-2049", "INT-2052"]
                    .iter()
                    .map(|id| call("interventions_get", json!({ "id": id })))
                    .collect(),
            )
        }),
        // 5. Availability of the team next week.
        ScriptedStep::dynamic(|req| {
            assert!(last_tool_results(req).iter().all(|r| !r.is_error));
            ScriptedReply::calls(
                "",
                vec![call(
                    "team_availability",
                    json!({ "from": "2026-10-19", "to": "2026-10-23" }),
                )],
            )
        }),
        // 7. A first proposal with a conflict is rejected by the connector's checks…
        ScriptedStep::dynamic(|req| {
            let availability = parse_result(&last_tool_results(req)[0].content);
            assert_eq!(availability.as_array().unwrap().len(), 6); // both teams' technicians, leads included
            let mut items = marc_plan();
            items[1]["startTime"] = json!("09:00");
            items.remove(0);
            ScriptedReply::calls(
                "",
                vec![call(
                    "planning_propose",
                    json!({ "title": "Suivi semaine 43", "items": items }),
                )],
            )
        }),
        // …and corrected (8. coherence verified by the tool).
        ScriptedStep::dynamic(|req| {
            let result = &last_tool_results(req)[0];
            assert!(result.is_error);
            assert!(result.content.contains("chevauche « INT-2061"));
            ScriptedReply::calls(
                "",
                vec![call(
                    "planning_propose",
                    json!({ "title": "Suivi semaine 43", "items": marc_plan() }),
                )],
            )
        }),
        // 9-10. Applying requires validation; a lesson is proposed for the team memory.
        ScriptedStep::dynamic(move |req| {
            let result = parse_result(&last_tool_results(req)[0].content);
            let id = result["propositionId"].as_str().unwrap().to_string();
            *captured.lock().unwrap() = id.clone();
            ScriptedReply::calls(
                "La proposition est prête. Je demande la validation pour l’appliquer.",
                vec![
                    call("planning_apply", json!({ "proposalId": id })),
                    call(
                        "memory_remember",
                        json!({
                            "space": "team",
                            "kind": "lesson",
                            "title": "Interventions « terminées » avec pièce en attente",
                            "content": "Vérifier le compte rendu : une pièce en attente impose le statut « en cours » et un suivi.",
                        }),
                    ),
                ],
            )
        }),
        // 6. Final synthesis, grounded in the records actually consulted.
        ScriptedStep::dynamic(|req| {
            let results = last_tool_results(req);
            assert_eq!(results.len(), 2);
            assert!(results.iter().all(|r| !r.is_error));
            ScriptedReply::text(
                [
                    "## Synthèse — semaine du 12 octobre",
                    "- Non terminées : INT-2042, INT-2046, INT-2049, INT-2051.",
                    "- Incohérences : INT-2043 (terminée avec pièce en attente), INT-2052 (non réalisée mais terminée).",
                    "- Planning de suivi appliqué : 6 créneaux la semaine prochaine.",
                ]
                .join("\n"),
            )
        }),
    ];
    let scenario = Scenario::new(user::MARC, steps);

    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = scenario.start().await
    else {
        panic!("expected a pause")
    };
    let names: Vec<&str> = approvals.iter().map(|a| a.tool_name.as_str()).collect();
    assert_eq!(names, ["planning_apply"]);
    assert!(scenario.store.applied().is_empty());
    // The memory proposal ran (draft risk) in the same turn and awaits human validation.
    let lesson = scenario
        .ports
        .memories()
        .into_iter()
        .find(|m| m.memory.kind == MemoryKind::Lesson)
        .unwrap();
    assert_eq!(lesson.memory.scope, MemoryScope::OrgUnit);
    assert_eq!(lesson.memory.org_unit_id, Some(unit::TECHNICAL));
    assert_eq!(lesson.memory.status, MemoryStatus::Proposed);

    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let RunOutcome::Completed { verification, .. } = scenario
        .runtime
        .resume(checkpoint, &decisions)
        .await
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(verification.passed);
    assert_eq!(scenario.store.applied().len(), 6);
    let id = proposal_id.lock().unwrap().parse().unwrap();
    assert_eq!(
        scenario.ports.deliverable(id).unwrap().status.as_str(),
        "applied"
    );
    assert_eq!(scenario.provider.remaining(), 0);

    // Traceability: every tool call is logged with its decision.
    let trace: Vec<String> = scenario
        .ports
        .tool_records()
        .iter()
        .map(|r| format!("{}:{}", r.tool_name, r.status))
        .collect();
    assert_eq!(
        trace,
        [
            "skill_read:succeeded",
            "interventions_search:succeeded",
            "interventions_get:succeeded",
            "interventions_get:succeeded",
            "interventions_get:succeeded",
            "team_availability:succeeded",
            "planning_propose:failed",
            "planning_propose:succeeded",
            "planning_apply:awaiting_approval",
            "memory_remember:succeeded",
            "planning_apply:succeeded",
        ]
    );
}

#[tokio::test]
async fn gives_a_team_lead_only_their_teams_data_for_the_same_request() {
    let scenario = Scenario::new(
        user::JULIEN,
        vec![
            ScriptedStep::calls("", vec![call("interventions_search", search_week())]),
            ScriptedStep::dynamic(|req| {
                let data = parse_result(&last_tool_results(req)[0].content);
                let ids: Vec<&str> = data["interventions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| i["id"].as_str().unwrap())
                    .collect();
                assert_eq!(
                    ids,
                    [
                        "INT-2041", "INT-2042", "INT-2043", "INT-2044", "INT-2045", "INT-2046",
                        "INT-2047"
                    ]
                );
                ScriptedReply::text("Équipe Nord : INT-2042 et INT-2046 restent à terminer.")
            }),
        ],
    );
    assert_eq!(scenario.start().await.status(), "completed");
}

#[tokio::test]
async fn exposes_no_intervention_data_to_the_technical_administrator() {
    let scenario = Scenario::new(
        user::PAUL,
        vec![
            ScriptedStep::dynamic(|req| {
                assert_eq!(
                    tool_names(req),
                    [
                        "document_read",
                        "document_search",
                        "memory_remember",
                        "memory_search",
                        "skill_read"
                    ]
                );
                ScriptedReply::calls("", vec![call("interventions_search", search_week())])
            }),
            ScriptedStep::text("Vous n’avez pas accès aux comptes rendus d’intervention."),
        ],
    );
    assert_eq!(scenario.start().await.status(), "completed");
    let first = &scenario.ports.tool_records()[0];
    assert_eq!(
        (first.tool_name.as_str(), first.status),
        ("interventions_search", "denied")
    );
}
