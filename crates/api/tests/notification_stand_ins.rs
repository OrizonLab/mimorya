//! The e-mails a stand-in receives (ADR-094, ADR-071): while a manager is
//! away, the notice and the reminder of the requests awaiting them go to
//! whom they named, as that colleague chose. The absent manager's own
//! choice silences no one; the stand-in's is kept: nothing sent, each
//! notice audited as refused, naming them, never tried again.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::delegations::{away, back, julien_decides_for_the_south};
use support::manager_approvals::{applying, due_in, route_to_managers};
use support::notification_choices::{audited, choose, chosen};
use support::{Client, TestApi, email};
use uuid::Uuid;

/// A request of Nadia's awaiting Marc, made with the mail relay recording.
async fn nadia_s_request(api: &TestApi, paul: &Client<'_>, start_time: &str) -> Value {
    let nadia = api.sign_in(user::NADIA).await;
    route_to_managers(paul, &["write"]).await;
    let (_, approval) = applying(api, &nadia, ("INT-2049", start_time), "Inchangé.").await;
    route_to_managers(paul, &[]).await;
    approval.expect("a request awaiting her manager")
}

fn id(approval: &Value) -> Uuid {
    approval["id"].as_str().unwrap().parse().unwrap()
}

async fn reject(api: &TestApi, decider: &Client<'_>, approval: &Value) {
    let decision = format!("/approvals/{}/decision", id(approval));
    let rejected = decider.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_stand_in_receives_the_e_mails_as_they_chose() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let julien = api.sign_in(user::JULIEN).await;
    let assignment = julien_decides_for_the_south(&paul).await;
    away(&marc, user::JULIEN, 0).await;
    let runs = &api.app.services.runs;
    runs.start_executing().unwrap();

    // Marc chose not to hear of them: Julien, standing in, still is.
    choose(&marc, chosen(false, true, true)).await;
    let first = nadia_s_request(&api, &paul, "09:00").await;
    let told = json!({ "approverId": user::MARC, "grouped": 1, "delegateId": user::JULIEN });
    let sent = [("success".to_string(), told)];
    assert_eq!(audited(&api, "approval.notified", &first).await, sent);
    due_in(&api, tenant::ID, id(&first), 120).await;
    assert_eq!(runs.remind_due().await.unwrap(), 1);
    assert_eq!(audited(&api, "approval.reminded", &first).await, sent);
    let mails = api.mail.sent();
    assert_eq!(mails.len(), 2, "{mails:?}");
    assert!(mails.iter().all(|mail| mail.to == email(user::JULIEN)));
    reject(&api, &julien, &first).await;

    // Julien chose not to: nobody is told, and it is not tried again.
    choose(&marc, chosen(true, true, true)).await;
    choose(&julien, chosen(false, true, true)).await;
    let second = nadia_s_request(&api, &paul, "11:00").await;
    let refused = json!({
        "approverId": user::MARC,
        "grouped": 1,
        "delegateId": user::JULIEN,
        "optedOut": true,
    });
    let not_sent = [("denied".to_string(), refused)];
    assert_eq!(audited(&api, "approval.notified", &second).await, not_sent);
    due_in(&api, tenant::ID, id(&second), 120).await;
    assert_eq!(runs.remind_due().await.unwrap(), 1);
    assert_eq!(runs.remind_due().await.unwrap(), 0);
    assert_eq!(audited(&api, "approval.reminded", &second).await, not_sent);
    assert_eq!(api.mail.sent().len(), 2);
    reject(&api, &julien, &second).await;

    choose(&julien, chosen(true, true, true)).await;
    back(&marc).await;
    let revoked = paul.delete(&assignment).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
}
