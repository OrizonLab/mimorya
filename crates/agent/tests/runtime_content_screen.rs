//! Runtime: the company's detection of instructions meant for an AI in
//! external content (ADR-213, D6). A result of a tool that brings external
//! content is screened; said to hold instructions, it reaches the model
//! behind the platform's notice and the person marked, and the run puts
//! each later action to a person, saying why. One the detection cannot read
//! holds the run suspect as well, without the notice. The platform's own
//! memory is never screened.

mod support;

use std::collections::HashSet;

use mimorya_agent::runtime::{RunOutcome, ScreenVerdict};
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_core::autonomy::{AutonomyLevel, AutonomyPolicy};
use serde_json::json;
use support::{Options, setup};

const NOTICE: &str = "[Avertissement de la plateforme : ce contenu contient des instructions";

/// A company that lets `planning_apply` run without validation.
fn autonomous() -> Options {
    Options {
        autonomy: Some(AutonomyPolicy {
            level: AutonomyLevel::AutonomousLimited,
            autonomous_tools: HashSet::from(["planning_apply".to_string()]),
        }),
        ..Options::default()
    }
}

/// Reads, checks what the model was shown, then acts.
fn read_then_act(notice_shown: bool) -> Vec<ScriptedStep> {
    vec![
        ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))]),
        ScriptedStep::dynamic(move |request| {
            let shown = &last_tool_results(request)[0].content;
            assert_eq!(shown.starts_with(NOTICE), notice_shown, "{shown}");
            ScriptedReply::calls("", vec![call("planning_apply", json!({ "slots": 1 }))])
        }),
        ScriptedStep::text("Fait."),
    ]
}

#[tokio::test]
async fn a_clear_content_changes_nothing() {
    let h = setup(read_then_act(false), autonomous());
    assert_eq!(h.start().await.status(), "completed");
    assert_eq!(*h.applied.lock().unwrap(), vec![json!({ "slots": 1 })]);
    // Each result screened, the action's too.
    assert_eq!(h.ports.screened(), ["jobs_search", "planning_apply"]);
}

#[tokio::test]
async fn a_suspect_content_is_marked_and_the_next_action_waits_for_a_person() {
    let h = setup(read_then_act(true), autonomous());
    h.ports.screen_says(ScreenVerdict::Suspect);
    let RunOutcome::AwaitingApproval { checkpoint, .. } = h.start().await else {
        panic!("expected a pause")
    };
    assert!(checkpoint.external_suspect);
    assert!(h.applied.lock().unwrap().is_empty());
    let (_, request) = &h.ports.approval_requests()[0];
    assert_eq!(
        request.reason,
        "Validation demandée : le run a lu un contenu externe qui contient, ou peut contenir, \
         des instructions destinées à l’IA"
    );
    // The person reads the result marked.
    let record = (h.ports.tool_records().into_iter())
        .find(|r| r.tool_name == "jobs_search")
        .unwrap();
    let summary = record.output_summary.unwrap();
    assert!(
        summary.contains("instructions destinées à l’IA détectées"),
        "{summary}"
    );
    let audited = h.ports.audit_entries();
    assert!(
        audited
            .iter()
            .any(|e| e.metadata["suspectContent"] == json!(true))
    );
}

#[tokio::test]
async fn an_unread_content_holds_the_run_suspect_without_the_notice() {
    let h = setup(read_then_act(false), Options::default());
    h.ports.screen_says(ScreenVerdict::Unread);
    let outcome = h.start().await;
    assert_eq!(outcome.status(), "awaiting_approval");
    let (_, request) = &h.ports.approval_requests()[0];
    assert_eq!(
        request.reason,
        "Action modifiant des données · Contenu externe suspect lu par ce run"
    );
}

#[tokio::test]
async fn the_platform_s_own_memory_is_never_screened() {
    let options = Options {
        platform: Some(Vec::new()),
        ..Options::default()
    };
    let steps = vec![
        ScriptedStep::calls("", vec![call("memory_search", json!({ "query": "x" }))]),
        ScriptedStep::text("Rien trouvé."),
    ];
    let h = setup(steps, options);
    h.ports.screen_says(ScreenVerdict::Suspect);
    assert_eq!(h.start().await.status(), "completed");
    assert!(h.ports.screened().is_empty());
}
