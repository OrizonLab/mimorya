//! Telling whom an absent manager named (ADR-071): while the absence runs,
//! the notice of a pause and the reminder the day before go to the stand-in,
//! naming in whose absence, provided the stand-in may decide the requests
//! (`approval:decide` over whom they are for); the audit names them. A
//! stand-in who could not decide or is disabled, or an absence not begun,
//! leaves the manager told, as before.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::delegations::{away, back, julien_decides_for_the_south};
use support::manager_approvals::{applying, due_in, route_to_managers};
use support::{Client, TestApi, email};
use uuid::Uuid;

/// A request of Nadia's awaiting Marc, made with the mail relay recording:
/// its identifier.
async fn nadia_s_request(api: &TestApi, paul: &Client<'_>, start_time: &str) -> Uuid {
    let nadia = api.sign_in(user::NADIA).await;
    route_to_managers(paul, &["write"]).await;
    let (_, approval) = applying(api, &nadia, ("INT-2049", start_time), "Inchangé.").await;
    route_to_managers(paul, &[]).await;
    let approval = approval.expect("a request awaiting her manager");
    approval["id"].as_str().unwrap().parse().unwrap()
}

/// The metadata of each `action` audited for the request.
async fn audited(api: &TestApi, approval: Uuid, action: &str) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<Value> = sqlx::query_scalar(
        "select metadata from audit_log where action = $1 and resource_id = $2 order by id",
    )
    .bind(action)
    .bind(approval.to_string())
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows
}

/// Marc was told of `approval`, the `count`th message sent.
async fn told_marc(api: &TestApi, approval: Uuid, count: usize) {
    let sent = api.mail.sent();
    assert_eq!(sent.len(), count, "{sent:?}");
    let mail = sent.last().unwrap();
    assert_eq!(mail.to, email(user::MARC));
    assert!(!mail.subject.contains("absence"), "{}", mail.subject);
    let manager = json!({ "approverId": user::MARC, "grouped": 1 });
    assert_eq!(audited(api, approval, "approval.notified").await, [manager]);
}

async fn reject(api: &TestApi, decider: &Client<'_>, approval: Uuid) {
    let decision = format!("/approvals/{approval}/decision");
    let rejected = decider.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_stand_in_is_told_and_reminded_in_the_manager_s_absence() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let julien = api.sign_in(user::JULIEN).await;
    let assignment = julien_decides_for_the_south(&paul).await;
    away(&marc, user::JULIEN, 0).await;

    let id = nadia_s_request(&api, &paul, "09:00").await;
    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::JULIEN));
    assert_eq!(
        sent[0].subject,
        "Une action de Nadia Benali attend votre validation (en l’absence de Marc Lefèvre)"
    );
    assert!(sent[0].text.starts_with("Bonjour Julien Moreau,"));
    let told = json!({ "approverId": user::MARC, "grouped": 1, "delegateId": user::JULIEN });
    assert_eq!(
        audited(&api, id, "approval.notified").await,
        std::slice::from_ref(&told)
    );

    // Reminded the day before, still in Marc's absence.
    due_in(&api, tenant::ID, id, 120).await;
    let runs = &api.app.services.runs;
    runs.claim_execution().await.unwrap();
    assert_eq!(runs.remind_due().await.unwrap(), 1);
    let sent = api.mail.sent();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(sent[1].to, email(user::JULIEN));
    assert!(sent[1].subject.starts_with("Rappel : "));
    assert_eq!(audited(&api, id, "approval.reminded").await, [told]);

    reject(&api, &julien, id).await;
    back(&marc).await;
    let revoked = paul.delete(&assignment).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_manager_is_told_when_the_stand_in_could_not_decide_is_disabled_or_not_yet_away() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;

    // Julien may not decide for Nadia: Marc is told, as before.
    away(&marc, user::JULIEN, 0).await;
    let first = nadia_s_request(&api, &paul, "11:00").await;
    told_marc(&api, first, 1).await;
    reject(&api, &marc, first).await;
    // Granted the right, but Marc's absence starts tomorrow: Marc again.
    let assignment = julien_decides_for_the_south(&paul).await;
    away(&marc, user::JULIEN, 1).await;
    let second = nadia_s_request(&api, &paul, "13:00").await;
    told_marc(&api, second, 2).await;
    reject(&api, &marc, second).await;
    // Standing in now, but disabled: Marc again.
    away(&marc, user::JULIEN, 0).await;
    let account = format!("/admin/users/{}", user::JULIEN);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    let third = nadia_s_request(&api, &paul, "15:00").await;
    told_marc(&api, third, 3).await;
    reject(&api, &marc, third).await;
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());

    back(&marc).await;
    let revoked = paul.delete(&assignment).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
}
