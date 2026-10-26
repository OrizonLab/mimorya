//! Runtime: human validation of side-effecting tool calls.

mod support;

use std::collections::HashMap;

use mimorya_agent::runtime::{ApprovalDecision, RunOutcome};
use mimorya_agent::testing::{ScriptedCall, ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_core::autonomy::ToolRisk;
use serde_json::json;
use support::{Options, setup};

fn with_id(name: &str, input: serde_json::Value, id: &str) -> ScriptedCall {
    ScriptedCall {
        id: Some(id.into()),
        ..call(name, input)
    }
}

#[tokio::test]
async fn pauses_for_human_validation_and_executes_exactly_the_validated_parameters() {
    let h = setup(
        vec![
            ScriptedStep::calls(
                "Je prépare l’application.",
                vec![
                    with_id("jobs_search", json!({ "query": "x" }), "c1"),
                    with_id("planning_apply", json!({ "slots": 3 }), "c2"),
                ],
            ),
            ScriptedStep::dynamic(|req| {
                let results = last_tool_results(req);
                let ids: Vec<&str> = results.iter().map(|r| r.tool_call_id.as_str()).collect();
                assert_eq!(ids, vec!["c1", "c2"]);
                assert!(results[1].content.contains("ok"));
                ScriptedReply::text("Planning appliqué pour INT-0001.")
            }),
        ],
        Options::default(),
    );
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause")
    };
    assert!(h.applied.lock().unwrap().is_empty());
    let (_, request) = &h.ports.approval_requests()[0];
    assert_eq!(request.tool_name, "planning_apply");
    assert_eq!(request.action_summary, "Appliquer 3 créneau(x)");
    assert_eq!(request.risk, ToolRisk::Write);

    let approval_id = approvals[0].approval_id;
    // Still waiting while no decision has been taken.
    let waiting = h
        .runtime
        .resume(checkpoint.clone(), &HashMap::new())
        .await
        .unwrap();
    assert_eq!(waiting.status(), "awaiting_approval");

    let decisions = HashMap::from([(approval_id, ApprovalDecision::approved())]);
    let second = h.runtime.resume(checkpoint, &decisions).await.unwrap();
    assert_eq!(second.status(), "completed");
    assert_eq!(*h.applied.lock().unwrap(), vec![json!({ "slots": 3 })]);
    let executed = h
        .ports
        .tool_records()
        .into_iter()
        .find(|r| r.tool_name == "planning_apply" && r.status == "succeeded")
        .unwrap();
    assert_eq!(executed.approval_id, Some(approval_id));
}

#[tokio::test]
async fn reports_a_rejected_validation_to_the_model() {
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![call("planning_apply", json!({ "slots": 1 }))]),
            ScriptedStep::dynamic(|req| {
                assert!(
                    last_tool_results(req)[0]
                        .content
                        .contains("refusée lors de la validation : pas cette semaine")
                );
                ScriptedReply::text("Entendu, je n’applique pas le planning.")
            }),
        ],
        Options::default(),
    );
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause")
    };
    let decisions = HashMap::from([(
        approvals[0].approval_id,
        ApprovalDecision::rejected(Some("pas cette semaine".into())),
    )]);
    let outcome = h.runtime.resume(checkpoint, &decisions).await.unwrap();
    assert_eq!(outcome.status(), "completed");
    assert!(h.applied.lock().unwrap().is_empty());
    // Ended in the live view, without what the user wrote.
    let ended = h.events_of("tool_result");
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0]["toolName"], "planning_apply");
    assert_eq!(ended[0]["summary"], "Action refusée lors de la validation");
    assert_eq!(ended[0]["isError"], true);
}

#[tokio::test]
async fn refuses_at_once_a_call_nobody_may_validate() {
    const NOBODY: &str = "Aucun responsable habilité ne peut valider cette action";
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![call("planning_apply", json!({ "slots": 1 }))]),
            ScriptedStep::dynamic(|req| {
                let result = &last_tool_results(req)[0];
                assert!(result.is_error);
                assert!(result.content.contains(NOBODY), "{}", result.content);
                ScriptedReply::text("Personne ne peut valider ce planning : je ne l’applique pas.")
            }),
        ],
        Options::default(),
    );
    h.ports.nobody_validates(NOBODY);
    // The run goes on without pausing: nothing waits for a decision.
    assert_eq!(h.start().await.status(), "completed");
    assert!(h.applied.lock().unwrap().is_empty());
    assert!(h.ports.approval_requests().is_empty());
    let ended = h.events_of("tool_result");
    assert_eq!(ended.len(), 1);
    assert_eq!(
        ended[0]["summary"],
        "Aucun responsable habilité à valider cette action"
    );
    assert_eq!(ended[0]["isError"], true);
    let logged = &h.ports.tool_records()[0];
    assert_eq!(
        (logged.tool_name.as_str(), logged.status),
        ("planning_apply", "denied")
    );
    let audited = h
        .ports
        .audit_entries()
        .into_iter()
        .find(|e| e.action == "agent.approval.unroutable")
        .expect("audited");
    assert_eq!(audited.reason.as_deref(), Some(NOBODY));
}

#[tokio::test]
async fn records_every_outcome_before_announcing_it() {
    let h = setup(
        vec![
            ScriptedStep::calls(
                "",
                vec![with_id("planning_apply", json!({ "slots": 1 }), "c1")],
            ),
            ScriptedStep::text("Planning appliqué."),
        ],
        Options::default(),
    );
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause")
    };
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    h.runtime.resume(checkpoint, &decisions).await.unwrap();

    // Clients reload on run_finished: the outcome must already be saved.
    let announced: Vec<(usize, String)> = h
        .ports
        .emitted()
        .into_iter()
        .map(|e| e.into_parts())
        .enumerate()
        .filter(|(_, (kind, _))| kind == "run_finished")
        .map(|(index, (_, data))| (index, data["status"].as_str().unwrap().to_string()))
        .collect();
    let statuses: Vec<&str> = announced.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(statuses, vec!["awaiting_approval", "completed"]);
    let finished = h.ports.finished();
    assert_eq!(
        finished.iter().map(|f| f.status).collect::<Vec<_>>(),
        statuses
    );
    for ((index, _), record) in announced.iter().zip(&finished) {
        assert!(record.events_before <= *index);
    }
}

#[tokio::test]
async fn never_runs_an_action_its_tool_no_longer_describes_as_validated() {
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![call("planning_apply", json!({ "slots": 3 }))]),
            ScriptedStep::dynamic(|req| {
                assert!(
                    last_tool_results(req)[0]
                        .content
                        .contains("Action modifiée depuis la demande de validation")
                );
                ScriptedReply::text("L’action n’a pas été appliquée.")
            }),
        ],
        Options::default(),
    );
    let RunOutcome::AwaitingApproval {
        approvals,
        mut checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause")
    };
    let pending = checkpoint.pending.as_mut().unwrap();
    assert_eq!(
        pending.approvals[0].action_summary.as_deref(),
        Some("Appliquer 3 créneau(x)")
    );
    // What the person read is no longer what the tool would do: its
    // declaration changed while the request waited.
    pending.approvals[0].action_summary = Some("Appliquer 2 créneau(x)".into());
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let outcome = h.runtime.resume(checkpoint, &decisions).await.unwrap();
    assert_eq!(outcome.status(), "completed");
    assert!(h.applied.lock().unwrap().is_empty());
    let ended = h.events_of("tool_result");
    assert_eq!(
        ended[0]["summary"],
        "Action modifiée depuis la demande de validation"
    );
    assert_eq!(ended[0]["isError"], true);
}
