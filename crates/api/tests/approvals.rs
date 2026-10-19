//! Decisions on approval requests: an expired request counts as a refusal, a
//! decision is only recorded while the run is actually paused for it, and only
//! by someone who may still use the agent it resumes.

mod support;

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::planning::{apply_proposal, first_result, propose};
use support::{Client, TestApi};
use uuid::Uuid;

struct PausedRun {
    run_id: String,
    approval_id: Uuid,
}

/// Scripts a proposal then its application, and returns the paused run.
async fn paused_run(
    api: &TestApi,
    marc: &Client<'_>,
    (intervention, start_time): (&str, &str),
    final_step: ScriptedStep,
) -> PausedRun {
    api.script(vec![
        propose(intervention, start_time),
        apply_proposal(""),
        final_step,
    ]);
    let conversation = marc.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let started = marc
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": format!("Planifie {intervention}") }),
        )
        .await
        .json();
    let run_id = started["runId"].as_str().unwrap().to_string();
    api.idle().await;
    let approvals = marc.get("/approvals").await.json();
    let approval = approvals["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["runId"] == run_id.as_str())
        .expect("approval requested")
        .clone();
    PausedRun {
        run_id,
        approval_id: approval["id"].as_str().unwrap().parse().unwrap(),
    }
}

async fn execute(api: &TestApi, sql: &str, id: Uuid) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(sql).bind(id).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

async fn statuses(marc: &Client<'_>, run_id: &str) -> (String, Vec<String>) {
    let detail = marc.get(&format!("/runs/{run_id}")).await.json();
    let approvals = detail["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["status"].as_str().unwrap().to_string())
        .collect();
    (
        detail["run"]["status"].as_str().unwrap().to_string(),
        approvals,
    )
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn reports_an_expired_validation_to_the_agent_as_a_refusal_and_resumes_the_run() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let seen = Arc::new(Mutex::new(String::new()));
    let observed = seen.clone();
    let final_step = ScriptedStep::dynamic(move |request| {
        *observed.lock().unwrap() = first_result(request);
        ScriptedReply::text(
            "La validation a expiré : le planning de INT-2052 n’a pas été appliqué.",
        )
    });
    let run = paused_run(&api, &marc, ("INT-2052", "15:00"), final_step).await;

    execute(
        &api,
        "update approvals set expires_at = now() - interval '1 minute' where id = $1",
        run.approval_id,
    )
    .await;
    let decision = marc
        .post(
            &format!("/approvals/{}/decision", run.approval_id),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(decision.status, StatusCode::CONFLICT);
    api.idle().await;

    assert!(seen.lock().unwrap().contains("expirée"));
    let (status, approvals) = statuses(&marc, &run.run_id).await;
    assert_eq!(status, "completed");
    assert_eq!(approvals, ["expired"]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn records_a_decision_only_while_the_run_is_paused_for_it() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let run = paused_run(
        &api,
        &marc,
        ("INT-2049", "09:00"),
        ScriptedStep::text("Le planning de INT-2049 est appliqué."),
    )
    .await;
    let run_id: Uuid = run.run_id.parse().unwrap();
    let decision_url = format!("/approvals/{}/decision", run.approval_id);
    let approve = json!({ "approved": true });

    // The run has not saved its pause yet: nothing would resume it.
    execute(
        &api,
        "update agent_runs set status = 'running' where id = $1",
        run_id,
    )
    .await;
    let early = marc.post(&decision_url, approve.clone()).await;
    assert_eq!(early.status, StatusCode::CONFLICT);
    assert!(early.error_message().contains("préparation"));

    // The run has ended in the meantime: the action can no longer be carried out.
    execute(
        &api,
        "update agent_runs set status = 'failed' where id = $1",
        run_id,
    )
    .await;
    let late = marc.post(&decision_url, approve.clone()).await;
    assert_eq!(late.status, StatusCode::CONFLICT);
    assert!(late.error_message().contains("terminée"));
    assert_eq!(statuses(&marc, &run.run_id).await.1, ["pending"]);

    // Once paused, the same decision is recorded and resumes the run.
    execute(
        &api,
        "update agent_runs set status = 'awaiting_approval' where id = $1",
        run_id,
    )
    .await;
    let accepted = marc.post(&decision_url, approve.clone()).await;
    assert_eq!(accepted.status, StatusCode::OK);
    assert_eq!(accepted.json()["resumed"], true);
    api.idle().await;

    let (status, approvals) = statuses(&marc, &run.run_id).await;
    assert_eq!(status, "completed");
    assert_eq!(approvals, ["approved"]);
    let replay = marc.post(&decision_url, approve).await;
    assert_eq!(replay.status, StatusCode::CONFLICT);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn deciding_resumes_the_agent_so_it_needs_agent_use() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let paul = api.sign_in(user::PAUL).await;
    let run = paused_run(
        &api,
        &marc,
        ("INT-2049", "11:00"),
        ScriptedStep::text("Le planning de INT-2049 est appliqué."),
    )
    .await;
    let only_marc = json!({ "op": "eq", "left": { "attr": "subject.userId" }, "right": { "value": user::MARC } });
    let created = paul
        .post(
            "/admin/policies",
            json!({ "description": "Plus d’agent pour Marc", "permissions": ["agent:use"], "condition": only_marc }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let policy = format!("/admin/policies/{}", created.json()["id"].as_str().unwrap());

    let decision_url = format!("/approvals/{}/decision", run.approval_id);
    let approve = json!({ "approved": true });
    let refused = marc.post(&decision_url, approve.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(statuses(&marc, &run.run_id).await.1, ["pending"]);

    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
    let accepted = marc.post(&decision_url, approve).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    api.idle().await;
    assert_eq!(statuses(&marc, &run.run_id).await.1, ["approved"]);
}
