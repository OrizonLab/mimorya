//! E-mailing a scheduled task's author that its run awaits their validation
//! (ADR-082): once per pause, naming the task and its conversation, never
//! the actions nor their parameters; audited per request. Requests routed to
//! a manager are told to that manager alone; a run its author follows in a
//! conversation of theirs, nobody. A notice a stop of the service
//! lost is sent at startup, once.

mod support;

use axum::http::StatusCode;
use chrono::Duration;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::approval_notices::approval_notices;
use mimorya_db::repositories::scheduled_run_notices::author_notices;
use serde_json::{Value, json};
use support::manager_approvals::{applying, route_to_managers};
use support::planning::{apply_proposal, propose};
use support::recovery::restart;
use support::scheduled::{due, every_day, fire, listed, schedule};
use support::{Client, TestApi, WEB_ORIGIN, email};
use uuid::Uuid;

const TITLE: &str = "Point du lundi";

/// The notices audited for `approval`: for whom, outcome and metadata.
async fn notices(api: &TestApi, approval: &str) -> Vec<(Uuid, String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select on_behalf_of_user_id, outcome, metadata from audit_log \
         where action = 'approval.notified' and resource_id = $1 \
         and actor_type = 'system' and actor_user_id is null order by id",
    )
    .bind(approval)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// Has `api` fire Nadia's task `id`, its run asking to apply a planning,
/// and returns the conversation it ran in and the request it awaits.
async fn paused(api: &TestApi, nadia: &Client<'_>, id: Uuid) -> (String, Value) {
    api.script(vec![
        propose("INT-2049", "09:00"),
        apply_proposal(""),
        ScriptedStep::text("Inchangé."),
    ]);
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(api).await, 1);
    let conversation = listed(nadia, id).await["lastConversationId"]
        .as_str()
        .unwrap()
        .to_string();
    let approvals = nadia.get("/approvals?status=pending").await.json();
    let approval = approvals["approvals"][0].clone();
    assert!(approval.is_object(), "{approvals}");
    (conversation, approval)
}

/// Refuses `approval` as `decider`, ending its run.
async fn refuse(api: &TestApi, decider: &Client<'_>, approval: &Value) {
    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let refused = decider.post(&decision, json!({ "approved": false })).await;
    assert_eq!(refused.status, StatusCode::OK, "{}", refused.text());
    api.idle().await;
}

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
        notices(&api, approval["id"].as_str().unwrap()).await,
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
async fn a_notice_a_stop_lost_is_sent_at_startup_once() {
    // Paused by a process that could not e-mail: what a stop between the
    // pause and its notice leaves.
    let stopped = TestApi::start().await;
    let nadia = stopped.sign_in(user::NADIA).await;
    let id = schedule(&nadia, every_day(TITLE)).await;
    let (conversation, approval) = paused(&stopped, &nadia, id).await;
    assert!(
        notices(&stopped, approval["id"].as_str().unwrap())
            .await
            .is_empty()
    );

    let started = TestApi::start_with_mailer().await;
    assert_eq!(restart(&started).await, 0, "no run was running");
    let sent = started.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::NADIA));
    assert!(sent[0].text.contains(&format!("/chat/{conversation}")));
    restart(&started).await;
    assert_eq!(started.mail.sent().len(), 1);

    refuse(&stopped, &nadia, &approval).await;
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
