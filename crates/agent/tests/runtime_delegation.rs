//! Runtime: tasks handed to sub-agents (ADR-098). A child run reads with
//! the tools that only read and run without validation, within its
//! parent's ceiling, and returns a summary whose sources its parent may
//! cite; it delegates nothing further.

mod support;

use std::collections::{HashMap, HashSet};

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::{RunOutcome, SUB_AGENT_BUDGET, SubAgentProfile};
use mimorya_agent::testing::{
    RecordedRequest, ScriptedCall, ScriptedReply, ScriptedStep, call, last_tool_results,
};
use mimorya_agent::tools::ToolOverride;
use serde_json::json;
use support::{Options, setup};

const TASK: &str = "Lister les interventions de l’équipe Nord";
const CONTEXT: &str = "Pour la synthèse hebdomadaire";
const ROLE: &str = "## Rôle : sous-agent";

fn delegate(profile: &str) -> ScriptedCall {
    call(
        "delegate_task",
        json!({ "profile": profile, "task": TASK, "context": CONTEXT }),
    )
}

fn turn_of(request: &RecordedRequest) -> String {
    match request.last_message() {
        Some(LlmMessage::User { content }) => content.clone(),
        other => panic!("expected the task, got {other:?}"),
    }
}

#[tokio::test]
async fn a_sub_agent_returns_its_summary_and_sources_to_its_parent() {
    let h = setup(
        vec![
            ScriptedStep::calls("Je confie la recherche.", vec![delegate("research")]),
            ScriptedStep::dynamic(|req| {
                assert!(
                    req.system
                        .starts_with("SYSTEM\n\n## Rôle : sous-agent « Recherche »")
                );
                // Reading only, without validation, no further delegation.
                assert_eq!(req.tool_names(), vec!["jobs_search", "doc_read"]);
                let turn = turn_of(req);
                assert!(turn.contains(TASK) && turn.contains(CONTEXT), "{turn}");
                ScriptedReply::calls("", vec![call("jobs_search", json!({ "query": "x" }))])
            }),
            ScriptedStep::text("INT-0001 : chaudière remise en service."),
            ScriptedStep::dynamic(|req| {
                assert!(!req.system.contains(ROLE));
                let results = last_tool_results(req);
                assert!(!results[0].is_error);
                assert!(results[0].content.contains("\"resume\""));
                assert!(results[0].content.contains("chaudière remise en service"));
                ScriptedReply::text("Synthèse : INT-0001 a remis la chaudière en service.")
            }),
        ],
        Options {
            sub_agents: true,
            ..Options::default()
        },
    );
    let RunOutcome::Completed {
        answer,
        verification,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected the parent's answer");
    };
    // INT-0001 is grounded by the sources the sub-agent read.
    assert!(verification.passed, "{:?}", verification.issues);
    assert!(answer.contains("INT-0001"));
    // The child's tokens count in the parent's budget, its steps do not.
    assert_eq!(checkpoint.usage.steps, 2);
    assert_eq!(checkpoint.usage.input_tokens, 400);
    assert_eq!(checkpoint.usage.output_tokens, 80);
    assert_eq!(checkpoint.usage.tool_calls, 1);

    let opened = h.ports.sub_runs();
    assert_eq!(opened.len(), 1);
    let child = &opened[0];
    assert_eq!(child.request.profile, SubAgentProfile::Research);
    assert_eq!(child.request.objective, TASK);
    assert_eq!(child.request.allowed_permissions, None);
    assert_eq!(child.request.budget.max_steps, SUB_AGENT_BUDGET.max_steps);
    assert_eq!(child.ports.finished()[0].status, "completed");
    let read: Vec<_> = child.ports.tool_records();
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].tool_name, "jobs_search");

    let record = h
        .ports
        .tool_records()
        .into_iter()
        .find(|r| r.tool_name == "delegate_task")
        .expect("the delegation's record");
    assert_eq!(record.status, "succeeded");
    assert_eq!(
        record.output_summary.as_deref(),
        Some("Sous-agent « Recherche » : résumé rendu")
    );
    let audited = h
        .ports
        .audit_entries()
        .into_iter()
        .find(|e| e.resource_id.as_deref() == Some("delegate_task"))
        .expect("the delegation's audit");
    assert_eq!(audited.metadata["subRunId"], json!(child.run_id));
    assert_eq!(audited.metadata["profile"], "research");
}

#[tokio::test]
async fn a_sub_agent_reads_within_its_parent_s_ceiling_and_delegates_nothing() {
    let h = setup(
        vec![
            ScriptedStep::dynamic(|req| {
                // Without intervention:read, the parent sees no search…
                assert!(!req.tool_names().contains(&"jobs_search".to_string()));
                ScriptedReply::calls("", vec![delegate("analysis")])
            }),
            ScriptedStep::dynamic(|req| {
                // …and neither does its sub-agent.
                assert_eq!(req.tool_names(), vec!["doc_read"]);
                ScriptedReply::calls(
                    "",
                    vec![
                        call("jobs_search", json!({ "query": "x" })),
                        call("planning_apply", json!({ "slots": 1 })),
                        delegate("research"),
                    ],
                )
            }),
            ScriptedStep::dynamic(|req| {
                let results = last_tool_results(req);
                assert!(results.iter().all(|r| r.is_error));
                assert!(
                    results[0]
                        .content
                        .contains("Permission intervention:read requise")
                );
                assert!(results[1].content.contains("planning_apply"));
                assert!(results[2].content.contains("delegate_task"));
                ScriptedReply::text("Aucune donnée accessible.")
            }),
            ScriptedStep::text("Bilan : aucune donnée accessible."),
        ],
        Options {
            sub_agents: true,
            allowed_permissions: Some(HashSet::from(["agent:use".to_string()])),
            ..Options::default()
        },
    );
    assert!(matches!(h.start().await, RunOutcome::Completed { .. }));
    let opened = h.ports.sub_runs();
    assert_eq!(opened.len(), 1);
    let child = &opened[0];
    assert_eq!(
        child.request.allowed_permissions,
        Some(vec!["agent:use".to_string()])
    );
    let refused: Vec<_> = child
        .ports
        .tool_records()
        .into_iter()
        .map(|r| (r.tool_name, r.status))
        .collect();
    assert_eq!(
        refused,
        vec![
            ("jobs_search".to_string(), "denied"),
            ("planning_apply".to_string(), "denied"),
            ("delegate_task".to_string(), "denied"),
        ]
    );
    assert!(child.ports.sub_runs().is_empty());
    assert!(child.ports.approval_requests().is_empty());
}

#[tokio::test]
async fn a_tool_that_needs_a_validation_is_not_a_sub_agent_s() {
    let overrides = HashMap::from([(
        "doc_read".to_string(),
        ToolOverride {
            requires_approval: Some(true),
            ..ToolOverride::default()
        },
    )]);
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![delegate("quality")]),
            ScriptedStep::dynamic(|req| {
                assert_eq!(req.tool_names(), vec!["jobs_search"]);
                ScriptedReply::calls("", vec![call("doc_read", json!({}))])
            }),
            ScriptedStep::dynamic(|req| {
                let results = last_tool_results(req);
                assert!(results[0].is_error);
                assert!(results[0].content.contains("inconnu ou désactivé"));
                ScriptedReply::text("Relecture faite : rien à corriger.")
            }),
            ScriptedStep::text("La relecture ne relève rien."),
        ],
        Options {
            sub_agents: true,
            overrides,
            ..Options::default()
        },
    );
    assert!(matches!(h.start().await, RunOutcome::Completed { .. }));
    let child = &h.ports.sub_runs()[0];
    assert!(child.ports.approval_requests().is_empty());
    assert_eq!(child.ports.finished()[0].status, "completed");
}
