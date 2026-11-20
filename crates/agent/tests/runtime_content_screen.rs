//! Runtime: the company's detection of instructions meant for an AI in
//! external content (ADR-213, D6). A result of a tool that brings external
//! content is screened; said to hold instructions, it reaches the model
//! behind the platform's notice and the person marked, and the run puts
//! each later action to a person, saying why. One the detection cannot read
//! holds the run suspect as well, without the notice; so does a sub-agent
//! that read a suspect or unreadable one, its report marked only when
//! instructions were detected. A suspect run stays so once resumed,
//! lifts no refusal and lets reads and drafts run. The platform's own
//! memory is never screened.

mod support;

use std::collections::{HashMap, HashSet};

use mimorya_agent::runtime::{ApprovalDecision, RunCheckpoint, RunOutcome, ScreenVerdict};
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

#[tokio::test]
async fn a_suspect_run_stays_so_once_resumed_and_lifts_nothing() {
    let mut options = autonomous();
    options.platform = Some(Vec::new());
    let steps = vec![
        ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))]),
        ScriptedStep::calls("", vec![call("planning_apply", json!({ "slots": 1 }))]),
        // Resumed: a read and a draft run as they did, the next action
        // waits again.
        ScriptedStep::calls(
            "",
            vec![
                call("jobs_search", json!({ "query": "y" })),
                call(
                    "memory_remember",
                    json!({ "space": "personal", "kind": "fact", "title": "Note",
                            "content": "À retenir" }),
                ),
                call("planning_apply", json!({ "slots": 2 })),
            ],
        ),
    ];
    let h = setup(steps, options);
    h.ports.screen_says(ScreenVerdict::Suspect);
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause")
    };
    // Kept through its stored form.
    let stored = serde_json::to_value(&checkpoint).unwrap();
    let checkpoint: RunCheckpoint = serde_json::from_value(stored).unwrap();
    assert!(checkpoint.external_suspect);
    // Nothing read after the resumption is suspect: the run stays so of
    // itself.
    h.ports.screen_says(ScreenVerdict::Clear);
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let resumed = h.runtime.resume(checkpoint, &decisions).await.unwrap();
    assert_eq!(resumed.status(), "awaiting_approval");
    assert_eq!(*h.applied.lock().unwrap(), vec![json!({ "slots": 1 })]);
    let requests = h.ports.approval_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].1.tool_name, "planning_apply");
    assert!(
        requests[1].1.reason.contains("contenu externe"),
        "{}",
        requests[1].1.reason
    );
    // The read and the draft ran, without a validation.
    let ran: Vec<String> = (h.ports.tool_records().into_iter())
        .filter(|r| r.status == "succeeded")
        .map(|r| r.tool_name)
        .collect();
    assert!(
        ran.iter().filter(|n| *n == "jobs_search").count() == 2,
        "{ran:?}"
    );
    assert!(ran.contains(&"memory_remember".to_string()), "{ran:?}");
}

#[tokio::test]
async fn a_suspect_run_lifts_no_refusal() {
    // It reads; it may not disable accounts, which it would do alone.
    let mut options = Options {
        autonomy: Some(AutonomyPolicy {
            level: AutonomyLevel::AutonomousLimited,
            autonomous_tools: HashSet::from(["users_disable".to_string()]),
        }),
        ..Options::default()
    };
    options.allowed_permissions = Some(HashSet::from(["intervention:read".to_string()]));
    let steps = vec![
        ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))]),
        ScriptedStep::calls("", vec![call("users_disable", json!({ "userId": "u-1" }))]),
        ScriptedStep::text("Je ne peux pas désactiver ce compte."),
    ];
    let h = setup(steps, options);
    h.ports.screen_says(ScreenVerdict::Suspect);
    let outcome = h.start().await;
    assert_eq!(outcome.status(), "completed");
    // Suspect, its refusal stays a refusal: no validation asked.
    assert!(outcome.checkpoint().external_suspect);
    assert_eq!(h.ports.screened(), ["jobs_search"]);
    assert!(h.ports.approval_requests().is_empty());
    let denied = (h.ports.tool_records().into_iter())
        .find(|r| r.tool_name == "users_disable")
        .unwrap();
    assert_eq!(denied.decision, "deny");
}

#[tokio::test]
async fn a_sub_agent_that_read_a_suspect_content_holds_its_parent_suspect() {
    let mut options = autonomous();
    options.sub_agents = true;
    let delegate = call(
        "delegate_task",
        json!({ "profile": "research", "task": "Lire les interventions" }),
    );
    let steps = vec![
        ScriptedStep::calls("", vec![delegate]),
        // The sub-agent reads, then reports.
        ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))]),
        ScriptedStep::text("Une intervention à reprendre."),
        // Its parent reads the report behind the notice, then acts.
        ScriptedStep::dynamic(|request| {
            let shown = &last_tool_results(request)[0].content;
            assert!(shown.starts_with(NOTICE), "{shown}");
            ScriptedReply::calls("", vec![call("planning_apply", json!({ "slots": 1 }))])
        }),
    ];
    let h = setup(steps, options);
    h.ports.screen_says(ScreenVerdict::Suspect);
    let RunOutcome::AwaitingApproval { checkpoint, .. } = h.start().await else {
        panic!("expected a pause")
    };
    assert!(checkpoint.external_suspect);
    assert!(h.applied.lock().unwrap().is_empty());
    let (_, request) = &h.ports.approval_requests()[0];
    assert!(
        request.reason.contains("contenu externe"),
        "{}",
        request.reason
    );
    // The report is marked for the person.
    let record = (h.ports.tool_records().into_iter())
        .find(|r| r.tool_name == "delegate_task")
        .unwrap();
    let summary = record.output_summary.unwrap();
    assert!(
        summary.contains("instructions destinées à l’IA détectées"),
        "{summary}"
    );
}

#[tokio::test]
async fn a_sub_agent_whose_reading_failed_holds_its_parent_suspect_unmarked() {
    let mut options = autonomous();
    options.sub_agents = true;
    let delegate = call(
        "delegate_task",
        json!({ "profile": "research", "task": "Lire les interventions" }),
    );
    let steps = vec![
        ScriptedStep::calls("", vec![delegate]),
        ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))]),
        ScriptedStep::text("Une intervention à reprendre."),
        ScriptedStep::dynamic(|request| {
            let shown = &last_tool_results(request)[0].content;
            assert!(!shown.starts_with(NOTICE), "{shown}");
            ScriptedReply::calls("", vec![call("planning_apply", json!({ "slots": 1 }))])
        }),
    ];
    let h = setup(steps, options);
    h.ports.screen_says(ScreenVerdict::Unread);
    let RunOutcome::AwaitingApproval { checkpoint, .. } = h.start().await else {
        panic!("expected a pause")
    };
    assert!(checkpoint.external_suspect && !checkpoint.instructions_detected);
    let record = (h.ports.tool_records().into_iter())
        .find(|r| r.tool_name == "delegate_task")
        .unwrap();
    assert!(!record.output_summary.unwrap().contains("instructions"));
}
