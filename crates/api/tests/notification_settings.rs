//! The e-mails each person chooses to receive (ADR-094): their own choice,
//! all e-mails by default, audited. A manager who chose not to hear of the
//! requests awaiting them is neither told, caught up with nor reminded, the
//! author of a mission's stage awaiting them is not told, nor of its end:
//! each notice audited as refused, never tried again. Choosing them again,
//! the next ones are sent. A stand-in's choice: `notification_stand_ins.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::manager_approvals::{applying, catch_up, due_in, paused_ago, route_to_managers};
use support::mission_notices::{awaiting, end_notices, end_notified, tell_ends, told};
use support::missions::{approving_review, entrust, executing, read, stop};
use support::notification_choices::{NOTIFICATIONS, audited, choose, chosen};
use support::queue::take_queued;
use support::{Client, TestApi, email};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_person_chooses_their_own_e_mails() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let nadia = api.sign_in(user::NADIA).await;
    assert_eq!(
        marc.get(NOTIFICATIONS).await.json(),
        chosen(true, true, true)
    );

    choose(&marc, chosen(false, true, false)).await;
    assert_eq!(
        marc.get(NOTIFICATIONS).await.json(),
        chosen(false, true, false)
    );
    // Another's choice stays theirs.
    assert_eq!(
        nadia.get(NOTIFICATIONS).await.json(),
        chosen(true, true, true)
    );
    let unknown = marc
        .put(NOTIFICATIONS, json!({ "approvals": true, "ownRuns": true, "missionEnds": true, "userId": user::NADIA }))
        .await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    let partial = marc.put(NOTIFICATIONS, json!({ "approvals": true })).await;
    assert_eq!(partial.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        nadia.get(NOTIFICATIONS).await.json(),
        chosen(true, true, true)
    );

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let changes: Vec<(Uuid, Value)> = sqlx::query_as(
        "select actor_user_id, metadata from audit_log \
         where action = 'account.notifications_configured' order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        changes.last().unwrap(),
        &(user::MARC, chosen(false, true, false))
    );
    choose(&marc, chosen(true, true, true)).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_manager_who_chose_not_to_hear_of_requests_is_neither_told_nor_reminded() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;
    choose(&marc, chosen(false, true, true)).await;
    route_to_managers(&paul, &["write"]).await;
    let (_, approval) = applying(&api, &nadia, ("INT-2049", "11:00"), "Inchangé.").await;
    let approval = approval.expect("a request awaiting her manager");
    assert!(api.mail.sent().is_empty());
    let refused = json!({ "approverId": user::MARC, "grouped": 1, "optedOut": true });
    assert_eq!(
        audited(&api, "approval.notified", &approval).await,
        [("denied".to_string(), refused.clone())]
    );

    // Neither caught up as lost nor reminded the day before.
    let id: Uuid = approval["id"].as_str().unwrap().parse().unwrap();
    paused_ago(&api, tenant::ID, id, 11).await;
    assert_eq!(catch_up(&api).await, 0);
    assert_eq!(
        audited(&api, "approval.notified", &approval).await,
        [("denied".to_string(), refused.clone())]
    );
    due_in(&api, tenant::ID, id, 120).await;
    assert_eq!(api.app.services.runs.remind_due().await.unwrap(), 1);
    api.idle().await;
    assert!(api.mail.sent().is_empty());
    assert_eq!(
        audited(&api, "approval.reminded", &approval).await,
        [("denied".to_string(), refused)]
    );
    assert_eq!(api.app.services.runs.remind_due().await.unwrap(), 0);
    reject(&api, &marc, &approval).await;

    // Chosen again, the next request is told.
    choose(&marc, chosen(true, true, true)).await;
    let (_, next) = applying(&api, &nadia, ("INT-2049", "11:00"), "Inchangé.").await;
    route_to_managers(&paul, &[]).await;
    let next = next.expect("another request awaiting her manager");
    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::MARC));
    reject(&api, &marc, &next).await;
}

/// Refuses `request`, as its manager.
async fn reject(api: &TestApi, marc: &Client<'_>, request: &Value) {
    let decision = format!("/approvals/{}/decision", request["id"].as_str().unwrap());
    let rejected = marc.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_author_of_a_mission_hears_of_it_only_as_they_chose() {
    let api = TestApi::start_with_mailer().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    choose(&marc, chosen(true, false, false)).await;

    // A stage awaiting him: not told, audited as refused, not caught up.
    let waiting = "Mission en attente, sans e-mail";
    let (id, _, approval) = awaiting(&api, &marc, waiting).await;
    let refused = [(
        "denied".to_string(),
        json!({ "approverId": user::MARC, "grouped": 1, "optedOut": true }),
    )];
    assert!(told(&api, waiting).is_empty());
    assert_eq!(audited(&api, "approval.notified", &approval).await, refused);
    let approval_id: Uuid = approval["id"].as_str().unwrap().parse().unwrap();
    paused_ago(&api, tenant::ID, approval_id, 11).await;
    assert_eq!(catch_up(&api).await, 0);
    assert!(told(&api, waiting).is_empty());
    assert_eq!(audited(&api, "approval.notified", &approval).await, refused);
    assert_eq!(stop(&marc, id).await, StatusCode::OK);

    // Its end: not told, recorded, never tried again.
    let over = "Mission terminée, sans e-mail";
    let done = entrust(&marc, over).await;
    api.script(vec![
        mimorya_agent::testing::ScriptedStep::text("Synthèse : rien à signaler."),
        approving_review(),
    ]);
    assert_eq!(take_queued(&api).await, 1);
    assert_eq!(read(&marc, done).await["status"], "done");
    tell_ends(&api).await;
    tell_ends(&api).await;
    assert!(told(&api, over).is_empty());
    assert!(end_notified(done).await.is_some());
    assert_eq!(
        end_notices(done).await,
        [(
            "denied".to_string(),
            json!({ "status": "done", "optedOut": true })
        )]
    );
    choose(&marc, chosen(true, true, true)).await;
}
