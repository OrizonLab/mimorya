//! E-mailing the manager requests await (ADR-060): only requests routed to a
//! manager, to that manager, once per pause of the run, naming whom they are
//! for and where to decide, never the actions nor their parameters; audited
//! either way, each request notified once, and a message the relay refuses
//! leaves the requests waiting.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::approval_notices::approval_notices;
use mimorya_db::repositories::approvals::list_approvals_for_run;
use mimorya_db::repositories::runs::{RunStatus, get_run};
use serde_json::{Value, json};
use support::manager_approvals::{ROUTING, applying, route_to_managers};
use support::planning::{apply_proposal_times, apply_reply, propose};
use support::sweeps::sweep_from_elsewhere;
use support::{TestApi, WEB_ORIGIN, email};
use uuid::Uuid;

/// The notices audited for the run: outcome and metadata, in order. Each is
/// the platform's, for whom the agent acts (Nadia, in these tests).
async fn notices(api: &TestApi, run_id: &str) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<(String, Value)> = sqlx::query_as(
        "select outcome, metadata from audit_log \
         where run_id = $1::uuid and action = 'approval.notified' \
         and actor_type = 'system' and actor_user_id is null \
         and on_behalf_of_user_id = $2 order by id",
    )
    .bind(run_id)
    .bind(user::NADIA)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows
}

/// Whom a notice of `approval` would go to now, if anyone.
async fn recipient(api: &TestApi, approval: &Value) -> Option<String> {
    let id: Uuid = approval["id"].as_str().unwrap().parse().unwrap();
    let run: Uuid = approval["runId"].as_str().unwrap().parse().unwrap();
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let notices = approval_notices(&mut tx, run, &[id]).await.unwrap();
    notices.into_iter().next().map(|n| n.approver_email)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_manager_is_told_where_to_decide_never_what() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;

    // Decided by whom the agent acts for: nobody is e-mailed.
    let (_, own) = applying(&api, &nadia, ("INT-2049", "09:00"), "Inchangé.").await;
    let own = own.expect("a request she decides");
    let refused = nadia
        .post(
            &format!("/approvals/{}/decision", own["id"].as_str().unwrap()),
            json!({ "approved": false }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::OK);
    api.idle().await;
    assert!(api.mail.sent().is_empty());

    // With a relay, the administration says managers are e-mailed.
    assert_eq!(paul.get(ROUTING).await.json()["notifiesByEmail"], true);
    route_to_managers(&paul, &["write"]).await;
    let (run_id, approval) = applying(&api, &nadia, ("INT-2049", "11:00"), "Inchangé.").await;
    route_to_managers(&paul, &[]).await;
    let approval = approval.expect("a request awaiting her manager");

    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    let mail = &sent[0];
    assert_eq!(mail.to, email(user::MARC));
    assert_eq!(mail.to_name, "Marc Lefèvre");
    assert_eq!(
        mail.subject,
        "Une action de Nadia Benali attend votre validation"
    );
    assert!(mail.text.contains(&format!("\n{WEB_ORIGIN}/approvals\n")));
    assert!(mail.text.contains("(risque : modification)"));
    // Neither the action nor its parameters leave the platform.
    let summary = approval["actionSummary"].as_str().unwrap();
    let proposal = approval["payload"]["proposalId"].as_str().unwrap();
    for hidden in [summary, proposal, "INT-2049", "planning_apply"] {
        assert!(!mail.text.contains(hidden), "{hidden} in {}", mail.text);
        assert!(!mail.subject.contains(hidden));
    }
    assert_eq!(
        notices(&api, &run_id).await,
        [(
            "success".to_string(),
            json!({ "approverId": user::MARC, "grouped": 1 })
        )]
    );

    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let rejected = marc.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_message_the_relay_refuses_leaves_the_request_waiting() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;
    api.mail.fail();
    route_to_managers(&paul, &["write"]).await;
    let (run_id, approval) = applying(&api, &nadia, ("INT-2052", "11:00"), "Inchangé.").await;
    route_to_managers(&paul, &[]).await;

    // Recorded as failed; the request waits for Marc all the same.
    let approval = approval.expect("a request awaiting her manager");
    assert_eq!(approval["status"], "pending");
    assert_eq!(approval["approver"]["id"], json!(user::MARC));
    assert_eq!(
        notices(&api, &run_id).await,
        [(
            "failure".to_string(),
            json!({ "approverId": user::MARC, "grouped": 1 })
        )]
    );
    let detail = nadia.get(&format!("/runs/{run_id}")).await.json();
    assert_eq!(detail["run"]["status"], "awaiting_approval", "{detail}");

    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let rejected = marc.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_request_still_awaiting_an_active_manager_is_e_mailed() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;

    // Decided by whom the agent acts for: no manager to tell.
    let (_, own) = applying(&api, &nadia, ("INT-2052", "09:00"), "Inchangé.").await;
    let own = own.expect("a request she decides");
    assert_eq!(recipient(&api, &own).await, None);
    let decision = format!("/approvals/{}/decision", own["id"].as_str().unwrap());
    let refused = nadia.post(&decision, json!({ "approved": false })).await;
    assert_eq!(refused.status, StatusCode::OK);
    api.idle().await;

    route_to_managers(&paul, &["write"]).await;
    let (_, routed) = applying(&api, &nadia, ("INT-2052", "13:00"), "Inchangé.").await;
    route_to_managers(&paul, &[]).await;
    let routed = routed.expect("a request awaiting her manager");
    assert_eq!(recipient(&api, &routed).await, Some(email(user::MARC)));

    // Not to a disabled account...
    let account = format!("/admin/users/{}", user::MARC);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(recipient(&api, &routed).await, None);
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    assert_eq!(recipient(&api, &routed).await, Some(email(user::MARC)));

    // ...nor once decided (Marc signs in again: disabling ended his sessions).
    let marc = api.sign_in(user::MARC).await;
    let decision = format!("/approvals/{}/decision", routed["id"].as_str().unwrap());
    let rejected = marc.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
    assert_eq!(recipient(&api, &routed).await, None);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_requests_of_one_pause_make_one_message() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;
    route_to_managers(&paul, &["write"]).await;
    api.script(vec![
        propose("INT-2049", "14:00"),
        apply_proposal_times("", 2),
        ScriptedStep::text("Inchangé."),
    ]);
    let conversation = nadia.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let started = nadia
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": "Planifie INT-2049" }),
        )
        .await
        .json();
    let run_id = started["runId"].as_str().unwrap().to_string();
    api.idle().await;
    route_to_managers(&paul, &[]).await;

    // Two requests for Marc, one message, each request audited with it.
    let waiting: Vec<Value> = marc.get("/approvals").await.json()["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["runId"] == run_id.as_str())
        .cloned()
        .collect();
    assert_eq!(waiting.len(), 2);
    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(
        sent[0].subject,
        "2 actions de Nadia Benali attendent votre validation"
    );
    let grouped = (
        "success".to_string(),
        json!({ "approverId": user::MARC, "grouped": 2 }),
    );
    assert_eq!(notices(&api, &run_id).await, [grouped.clone(), grouped]);
    // Notified, a request is not sent again, should the run pause on it anew.
    for approval in &waiting {
        assert_eq!(recipient(&api, approval).await, None);
    }

    for approval in &waiting {
        let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
        let rejected = marc.post(&decision, json!({ "approved": false })).await;
        assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    }
    api.idle().await;
    // Taken up, the run told nobody again.
    assert_eq!(api.mail.sent().len(), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_ended_elsewhere_before_its_pause_makes_no_request_to_tell_of() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    route_to_managers(&paul, &["write"]).await;
    // While the model answers, another process fails the run as stuck: no
    // longer running under its lease, the step makes no request (ADR-086).
    api.script(vec![
        propose("INT-2052", "14:00"),
        ScriptedStep::dynamic(|request| {
            sweep_from_elsewhere(user::NADIA);
            apply_reply(request, "", 2)
        }),
    ]);
    let run_id = support::runs::start_run(&nadia).await;
    api.idle().await;
    route_to_managers(&paul, &[]).await;

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let run = get_run(&mut tx, run_id).await.unwrap().unwrap();
    let requests = list_approvals_for_run(&mut tx, run_id).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(run.status(), RunStatus::Failed);
    assert!(requests.is_empty(), "{requests:?}");
    assert!(api.mail.sent().is_empty(), "{:?}", api.mail.sent());
    assert!(notices(&api, &run_id.to_string()).await.is_empty());
}
