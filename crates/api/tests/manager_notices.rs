//! E-mailing the manager a request awaits (ADR-060): only a request routed to
//! a manager, to that manager, naming whom it is for and where to decide,
//! never the action nor its parameters; audited either way, and a message
//! the relay refuses leaves the request waiting.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::manager_approvals::{ROUTING, applying, route_to_managers};
use support::{TestApi, WEB_ORIGIN, email};

/// The notices audited for the run: outcome and metadata, in order.
async fn notices(api: &TestApi, run_id: &str) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<(String, Value)> = sqlx::query_as(
        "select outcome, metadata from audit_log \
         where run_id = $1::uuid and action = 'approval.notified' order by id",
    )
    .bind(run_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows
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
        [("success".to_string(), json!({ "approverId": user::MARC }))]
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
        [("failure".to_string(), json!({ "approverId": user::MARC }))]
    );
    let detail = nadia.get(&format!("/runs/{run_id}")).await.json();
    assert_eq!(detail["run"]["status"], "awaiting_approval", "{detail}");

    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let rejected = marc.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
}
