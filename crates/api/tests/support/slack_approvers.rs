//! Requests awaiting a manager told in Slack in the tests (ADR-130): a
//! request of Nadia's awaiting Marc, the pass telling of it, the direct
//! message a person is written, and what became of each notice.

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_api::services::gateways::{NOTIFIED_WITHIN, notify_approvers_in_slack};
use mimorya_connectors::testing::{FakeSlack, SlackPost};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::gateway_approval_notices::gateway_approvals_to_notify;
use serde_json::json;
use uuid::Uuid;

use super::gateways::dm_of;
use super::manager_approvals::{applying, route_to_managers};
use super::{Client, TestApi};

/// A request of Nadia's, made in the application, awaiting Marc, to plan
/// INT-2049 at `start_time`: its run and its identifier. The model's last
/// answer waits for its decision: one request at a time.
pub async fn nadia_s_request(api: &TestApi, paul: &Client<'_>, start_time: &str) -> (String, Uuid) {
    let nadia = api.sign_in(user::NADIA).await;
    route_to_managers(paul, &["write"]).await;
    let (run, approval) = applying(api, &nadia, ("INT-2049", start_time), "Inchangé.").await;
    route_to_managers(paul, &[]).await;
    let approval = approval.expect("a request awaiting her manager");
    (run, approval["id"].as_str().unwrap().parse().unwrap())
}

/// Tells in Slack the managers whom requests await, as a process that
/// executes runs; how many messages were sent.
pub async fn notify(api: &TestApi) -> usize {
    api.app.services.runs.start_executing().unwrap();
    notify_approvers_in_slack(&api.app.services).await.unwrap()
}

/// How many runs, of every company, the pass would pick now.
pub async fn to_notify(api: &TestApi) -> usize {
    let (now, pool) = (Utc::now(), api.app.services.db.pool());
    let runs = gateway_approvals_to_notify(pool, now - NOTIFIED_WITHIN, now, 20).await;
    runs.unwrap().len()
}

/// What the application wrote in the direct conversation of the Slack user
/// `person`, oldest first, the link offered when they first wrote aside.
pub fn direct_to(slack: &FakeSlack, person: &str) -> Vec<SlackPost> {
    let channel = dm_of(person);
    (slack.posts().into_iter())
        .filter(|p| p.channel == channel && !p.text.contains("gateway-link#"))
        .collect()
}

/// What became of the notice of `approval` in Slack: its outcome (none
/// while to try again) and attempts, if it was tried.
pub async fn notice_of(api: &TestApi, approval: Uuid) -> Option<(Option<String>, i32)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let row = sqlx::query_as(
        "select outcome, attempts from gateway_approval_notices where approval_id = $1",
    )
    .bind(approval)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    row
}

/// Makes the notices waiting to be tried again due.
pub async fn retry_notices_now(api: &TestApi) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update gateway_approval_notices set retry_after = now() - interval '1 second' \
         where outcome is null",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// Has `decider` refuse `approval` in the application: the tests leave no
/// request awaiting a manager behind them.
pub async fn refuse(api: &TestApi, decider: &Client<'_>, approval: Uuid) {
    let decision = format!("/approvals/{approval}/decision");
    let refused = decider.post(&decision, json!({ "approved": false })).await;
    assert_eq!(refused.status, StatusCode::OK, "{}", refused.text());
    api.idle().await;
}
