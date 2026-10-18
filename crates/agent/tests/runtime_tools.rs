//! Runtime: tool visibility, policy enforcement and untrusted tool output.

mod support;

use mimorya_agent::runtime::RunOutcome;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_core::demo::{self, user};
use serde_json::json;
use support::{Options, setup};

#[tokio::test]
async fn only_shows_the_model_the_tools_the_user_may_use_never_non_delegable_ones() {
    let paul = setup(
        vec![ScriptedStep::text("Bonjour")],
        Options {
            subject: Some(demo::subject(user::PAUL)),
            ..Default::default()
        },
    );
    paul.start().await;
    assert_eq!(paul.provider.requests()[0].tool_names(), vec!["doc_read"]);

    let julien = setup(vec![ScriptedStep::text("Bonjour")], Options::default());
    julien.start().await;
    let mut names = julien.provider.requests()[0].tool_names();
    names.sort();
    assert_eq!(names, vec!["doc_read", "jobs_search", "planning_apply"]);
}

#[tokio::test]
async fn runs_read_tools_with_the_users_data_scope_and_returns_grounded_answers() {
    let h = setup(
        vec![
            ScriptedStep::calls(
                "Je cherche.",
                vec![call("jobs_search", json!({ "query": "semaine" }))],
            ),
            ScriptedStep::dynamic(|req| {
                let content = &last_tool_results(req)[0].content;
                assert!(content.contains("fiabilite=\"non_fiable\""));
                assert!(content.contains("INT-0001"));
                assert!(!content.contains("INT-0002"));
                ScriptedReply::text("Une intervention : INT-0001.")
            }),
        ],
        Options::default(),
    );
    let RunOutcome::Completed {
        answer,
        verification,
        ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert!(verification.passed);
    assert_eq!(answer, "Une intervention : INT-0001.");
    assert_eq!(h.provider.remaining(), 0);
    let records: Vec<(String, &str)> = h
        .ports
        .tool_records()
        .into_iter()
        .map(|r| (r.tool_name, r.status))
        .collect();
    assert_eq!(records, vec![("jobs_search".to_string(), "succeeded")]);
    let actions: Vec<String> = h
        .ports
        .audit_entries()
        .into_iter()
        .map(|a| a.action)
        .collect();
    assert_eq!(actions, vec!["agent.tool.execute"]);
    assert_eq!(h.ports.usage_records().len(), 2);
    assert_eq!(
        h.ports.deltas(),
        vec!["Je cherche.", "Une intervention : INT-0001."]
    );
}

#[tokio::test]
async fn denies_calls_to_tools_the_user_cannot_use_even_if_the_model_insists() {
    let h = setup(
        vec![
            ScriptedStep::calls(
                "",
                vec![call("users_disable", json!({ "userId": user::LUCAS }))],
            ),
            ScriptedStep::dynamic(|req| {
                assert!(
                    last_tool_results(req)[0]
                        .content
                        .contains("Refusé par la plateforme")
                );
                ScriptedReply::text("Je ne peux pas désactiver de compte.")
            }),
        ],
        Options::default(),
    );
    assert_eq!(h.start().await.status(), "completed");
    let record = &h.ports.tool_records()[0];
    assert_eq!(
        (record.tool_name.as_str(), record.status),
        ("users_disable", "denied")
    );
    let audit = &h.ports.audit_entries()[0];
    assert_eq!(
        (audit.action.as_str(), audit.outcome),
        ("agent.tool.denied", "denied")
    );
}

#[tokio::test]
async fn neutralises_prompt_injection_carried_by_tool_results() {
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![call("doc_read", json!({}))]),
            ScriptedStep::dynamic(|req| {
                let content = &last_tool_results(req)[0].content;
                // The payload cannot close the untrusted envelope.
                assert_eq!(content.matches("</donnees_outil>").count(), 1);
                // A compromised model following the injected instruction is still stopped by the PEP.
                ScriptedReply::calls("", vec![call("users_disable", json!({ "userId": "all" }))])
            }),
            ScriptedStep::text(
                "Le document contenait une instruction suspecte que je n’ai pas suivie.",
            ),
        ],
        Options {
            subject: Some(demo::subject(user::PAUL)),
            ..Default::default()
        },
    );
    assert_eq!(h.start().await.status(), "completed");
    let disable = h
        .ports
        .tool_records()
        .into_iter()
        .find(|r| r.tool_name == "users_disable")
        .unwrap();
    assert_eq!(disable.status, "denied");
}

#[tokio::test]
async fn rejects_invalid_tool_inputs_and_lets_the_model_correct_them() {
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": 42 }))]),
            ScriptedStep::dynamic(|req| {
                assert!(
                    last_tool_results(req)[0]
                        .content
                        .contains("Paramètres invalides")
                );
                ScriptedReply::calls("", vec![call("jobs_search", json!({ "query": "ok" }))])
            }),
            ScriptedStep::text("INT-0001 est concernée."),
        ],
        Options::default(),
    );
    assert_eq!(h.start().await.status(), "completed");
    let statuses: Vec<&str> = h
        .ports
        .tool_records()
        .into_iter()
        .map(|r| r.status)
        .collect();
    assert_eq!(statuses, vec!["denied", "succeeded"]);
}
