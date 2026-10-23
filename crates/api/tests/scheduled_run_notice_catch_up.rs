//! Notices to scheduled tasks' authors a stop of the service lost (ADR-082,
//! ADR-070): at startup, the author of a request still awaiting them in the
//! paused run their task last started, never told of it, is told once, in
//! their own company; not while their account is disabled or their company
//! suspended, nor once the request has expired.

mod support;

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::scheduled_run_notices::unnotified_scheduled_runs;
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::manager_approvals::due_in;
use support::recovery::restart;
use support::scheduled::{every_day, schedule};
use support::scheduled_notices::{
    TITLE, end_foreign_scheduled_request, foreign_scheduled_request, notices, paused, refuse,
};
use support::{Client, TestApi, email, owner_url};
use uuid::Uuid;

/// Nadia's task, its run paused by a process that could not e-mail: what a
/// stop between the pause and its notice leaves. The task, its run's
/// conversation and the request.
async fn lost(stopped: &TestApi, nadia: &Client<'_>) -> (Uuid, String, Value) {
    let id = schedule(nadia, every_day(TITLE)).await;
    let (conversation, approval) = paused(stopped, nadia, id).await;
    let approval_id = approval["id"].as_str().unwrap();
    assert!(notices(stopped, tenant::ID, approval_id).await.is_empty());
    (id, conversation, approval)
}

/// Whether the startup lists `approval`'s run as one to catch up.
async fn listed_at_startup(api: &TestApi, approval: &Value) -> bool {
    let run: Uuid = approval["runId"].as_str().unwrap().parse().unwrap();
    let runs = unnotified_scheduled_runs(api.app.services.db.pool(), Utc::now())
        .await
        .unwrap();
    runs.iter()
        .any(|r| r.run_id == run && r.tenant_id == tenant::ID)
}

async fn end(stopped: &TestApi, nadia: &Client<'_>, id: Uuid, approval: &Value) {
    refuse(stopped, nadia, approval).await;
    let deleted = nadia.delete(&format!("/scheduled-tasks/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
}

/// Sets the demo company's status, as an operator would.
async fn company_status(status: &str) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("update tenants set status = $2 where id = $1")
        .bind(tenant::ID)
        .bind(status)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_notice_a_stop_lost_is_sent_at_startup_once() {
    let stopped = TestApi::start().await;
    let nadia = stopped.sign_in(user::NADIA).await;
    let (id, conversation, approval) = lost(&stopped, &nadia).await;

    let started = TestApi::start_with_mailer().await;
    assert_eq!(restart(&started).await, 0, "no run was running");
    let sent = started.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::NADIA));
    assert!(sent[0].text.contains(&format!("/chat/{conversation}")));
    restart(&started).await;
    assert_eq!(started.mail.sent().len(), 1);
    end(&stopped, &nadia, id, &approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nobody_is_told_for_a_disabled_author_or_a_suspended_company() {
    let stopped = TestApi::start().await;
    let paul = stopped.sign_in(user::PAUL).await;
    let nadia = stopped.sign_in(user::NADIA).await;
    let (id, _, approval) = lost(&stopped, &nadia).await;
    let started = TestApi::start_with_mailer().await;

    // Disabled after the pause: her requests stay, nobody is e-mailed.
    let account = format!("/admin/users/{}", user::NADIA);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    restart(&started).await;
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    assert!(started.mail.sent().is_empty());

    // Her company suspended: nobody either.
    company_status("suspended").await;
    let listed = listed_at_startup(&started, &approval).await;
    restart(&started).await;
    company_status("active").await;
    assert!(!listed);
    assert!(started.mail.sent().is_empty());

    // Both back: she is told.
    restart(&started).await;
    assert_eq!(started.mail.sent().len(), 1);
    let nadia = stopped.sign_in(user::NADIA).await;
    end(&stopped, &nadia, id, &approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_expired_request_is_not_caught_up() {
    let stopped = TestApi::start().await;
    let nadia = stopped.sign_in(user::NADIA).await;
    let (id, _, approval) = lost(&stopped, &nadia).await;
    let approval_id: Uuid = approval["id"].as_str().unwrap().parse().unwrap();
    due_in(&stopped, tenant::ID, approval_id, -1).await;

    let started = TestApi::start_with_mailer().await;
    assert!(!listed_at_startup(&started, &approval).await);
    restart(&started).await;
    assert!(started.mail.sent().is_empty());
    // Due again, to be refused.
    due_in(&stopped, tenant::ID, approval_id, 60).await;
    end(&stopped, &nadia, id, &approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_author_is_told_in_their_own_company() {
    let started = TestApi::start_with_mailer().await;
    let foreign = foreign_scheduled_request(&started).await;
    let request = foreign.2.to_string();

    restart(&started).await;
    let sent = started.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, "contact@autre-entreprise.example");
    assert!(sent[0].text.contains("(Autre entreprise)"));
    // No conversation: « Validations ».
    assert!(sent[0].text.contains("/approvals"), "{}", sent[0].text);
    assert_eq!(
        notices(&started, OTHER_TENANT_ID, &request).await,
        [(
            OTHER_USER_ID,
            "success".to_string(),
            json!({ "approverId": OTHER_USER_ID, "grouped": 1 })
        )]
    );
    assert!(notices(&started, tenant::ID, &request).await.is_empty());
    end_foreign_scheduled_request(&started, foreign).await;
}
