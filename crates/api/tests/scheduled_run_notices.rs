//! E-mailing a scheduled task's author that its run awaits their validation
//! (ADR-082): once per pause, naming the task and its conversation, never
//! the actions nor their parameters; audited per request. Requests routed to
//! a manager are told to that manager alone; a run its author follows in a
//! conversation of theirs, nobody. A message the relay refuses is audited,
//! and not sent again. What a stop of the service lost:
//! `scheduled_run_notice_catch_up.rs`.

mod support;

use axum::http::StatusCode;
use chrono::Duration;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::approval_notices::approval_notices;
use mimorya_db::repositories::author_notices::author_notices;
use serde_json::json;
use support::manager_approvals::{applying, route_to_managers};
use support::recovery::restart;
use support::scheduled::{due, every_day, fire, schedule};
use support::scheduled_notices::{TITLE, notices, paused, refuse};
use support::{TestApi, WEB_ORIGIN, email};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_author_is_told_once_where_their_task_waits() {
    let api = TestApi::start_with_mailer().await;
    let nadia = api.sign_in(user::NADIA).await;
    let id = schedule(&nadia, every_day(TITLE)).await;
    let (conversation, approval) = paused(&api, &nadia, id).await;

    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::NADIA));
    assert_eq!(
        sent[0].subject,
        "Votre tâche planifiée « Point du lundi » attend votre validation"
    );
    let link = format!("{WEB_ORIGIN}/chat/{conversation}");
    assert!(sent[0].text.contains(&link), "{}", sent[0].text);
    assert!(sent[0].text.contains("une action (risque : modification)"));
    assert!(!sent[0].text.contains("INT-2049"));
    assert_eq!(
        notices(&api, tenant::ID, approval["id"].as_str().unwrap()).await,
        [(
            user::NADIA,
            "success".to_string(),
            json!({ "approverId": user::NADIA, "grouped": 1 })
        )]
    );

    // Not again: neither at the next start, nor for the occurrence the
    // pause holds back.
    restart(&api).await;
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(&api).await, 0);
    assert_eq!(api.mail.sent().len(), 1);
    refuse(&api, &nadia, &approval).await;

    // A run she follows herself, in a conversation of hers: nothing.
    let (_, own) = applying(&api, &nadia, ("INT-2049", "11:00"), "Inchangé.").await;
    let own = own.expect("a request she decides");
    assert_eq!(api.mail.sent().len(), 1);
    refuse(&api, &nadia, &own).await;
    assert_eq!(
        nadia.delete(&format!("/scheduled-tasks/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_request_routed_to_a_manager_is_told_to_the_manager_alone() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&nadia, every_day(TITLE)).await;
    route_to_managers(&paul, &["write"]).await;
    let (_, approval) = paused(&api, &nadia, id).await;
    route_to_managers(&paul, &[]).await;

    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::MARC));
    assert_eq!(
        sent[0].subject,
        "Une action de Nadia Benali attend votre validation"
    );
    refuse(&api, &marc, &approval).await;
    assert_eq!(
        nadia.delete(&format!("/scheduled-tasks/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_author_is_told_only_of_the_requests_they_decide() {
    // Without a relay, nothing is notified: what each would be told of shows.
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&nadia, every_day(TITLE)).await;
    route_to_managers(&paul, &["write"]).await;
    let (_, approval) = paused(&api, &nadia, id).await;
    route_to_managers(&paul, &[]).await;

    let approval_id: Uuid = approval["id"].as_str().unwrap().parse().unwrap();
    let run_id: Uuid = approval["runId"].as_str().unwrap().parse().unwrap();
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let managers = approval_notices(&mut tx, run_id, &[approval_id])
        .await
        .unwrap();
    let author = author_notices(&mut tx, run_id, &[approval_id])
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(managers.len(), 1);
    assert!(author.is_empty(), "{author:?}");

    refuse(&api, &marc, &approval).await;
    assert_eq!(
        nadia.delete(&format!("/scheduled-tasks/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_message_the_relay_refuses_is_audited_and_not_sent_again() {
    let api = TestApi::start_with_mailer().await;
    let nadia = api.sign_in(user::NADIA).await;
    let id = schedule(&nadia, every_day(TITLE)).await;
    api.mail.fail();
    let (_, approval) = paused(&api, &nadia, id).await;
    let approval_id = approval["id"].as_str().unwrap();
    let refused = [(
        user::NADIA,
        "failure".to_string(),
        json!({ "approverId": user::NADIA, "grouped": 1 }),
    )];
    assert_eq!(notices(&api, tenant::ID, approval_id).await, refused);
    // Not retried at the next start: the request waits in « Validations ».
    restart(&api).await;
    assert_eq!(notices(&api, tenant::ID, approval_id).await, refused);
    assert!(api.mail.sent().is_empty());
    refuse(&api, &nadia, &approval).await;
    assert_eq!(
        nadia.delete(&format!("/scheduled-tasks/{id}")).await.status,
        StatusCode::OK
    );
}
