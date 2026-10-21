//! E-mailing the manager a request awaits (ADR-060): only a request routed to
//! a manager, to that manager, naming whom it is for and where to decide,
//! never the action nor its parameters; audited either way, and a message
//! the relay refuses leaves the request waiting.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::approval_notices::approval_notice;
use serde_json::{Value, json};
use support::manager_approvals::{ROUTING, applying, route_to_managers};
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
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let notice = approval_notice(&mut tx, id).await.unwrap();
    notice.map(|n| n.approver_email)
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
