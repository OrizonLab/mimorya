//! A person talking with the agent in Slack in the tests (ADR-127): their
//! Slack user linked to their account, what they write as Slack sends it,
//! outside any thread or in one, and the answers the pass writes back.

use axum::http::StatusCode;
use mimorya_api::services::gateways::reply_to_finished_runs;
use mimorya_connectors::testing::{FakeSlack, WORKSPACE};
use mimorya_core::demo::tenant;
use serde_json::json;
use uuid::Uuid;

use super::gateways::{LINK, declare_slack, direct_message, dm_of, envelope, offered_code, send};
use super::{Client, TestApi};

/// Marc's Slack user, and his direct conversation with the application.
pub const MARC_SLACK: &str = "U0MARCLEF";
pub const MARC_DM: &str = "D0MARCLEF";

/// The gateway, declared by `paul`, `person`'s Slack user (of `address`)
/// linked to `client`'s account.
pub async fn linked(
    api: &TestApi,
    slack: &FakeSlack,
    paul: &Client<'_>,
    client: &Client<'_>,
    (person, address): (&str, &str),
) -> Uuid {
    let gateway = declare_slack(paul).await;
    slack.add_person(person, address);
    let code = link_code(api, slack, gateway, person).await;
    confirm(client, &code).await;
    gateway
}

/// The code of the link the application sends `person`, not yet linked,
/// when they write to it.
pub async fn link_code(api: &TestApi, slack: &FakeSlack, gateway: Uuid, person: &str) -> String {
    let sent = send(
        api,
        gateway,
        &direct_message(&event_id(), person, "Bonjour"),
    )
    .await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    api.idle().await;
    offered_code(slack)
}

/// Links, as `client`, the Slack user the link `code` names.
pub async fn confirm(client: &Client<'_>, code: &str) {
    let confirmed = client.post(LINK, json!({ "code": code })).await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.text());
}

/// An event never sent before.
pub fn event_id() -> String {
    format!("Ev0{}", Uuid::new_v4().simple())
}

/// What `person` writes to the application at `ts`, in the thread `thread`
/// if any, as Slack sends it.
pub fn written(person: &str, text: &str, ts: &str, thread: Option<&str>) -> String {
    let mut event = json!({
        "type": "message",
        "channel": dm_of(person),
        "user": person,
        "text": text,
        "ts": ts,
        "channel_type": "im",
        "event_ts": ts
    });
    if let Some(thread) = thread {
        event["thread_ts"] = json!(thread);
    }
    envelope(WORKSPACE, &event_id(), event)
}

/// Sends what was written, then waits for what it started.
pub async fn write(api: &TestApi, gateway: Uuid, body: &str) {
    let sent = send(api, gateway, body).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    settle(api).await;
}

/// Waits for the message answered and the run it started.
pub async fn settle(api: &TestApi) {
    api.app.services.background.idle().await;
    api.idle().await;
}

/// Writes the runs' ends and pauses in Slack, as a process that executes
/// runs; how many were sent.
pub async fn answer(api: &TestApi) -> usize {
    api.app.services.runs.start_executing().unwrap();
    reply_to_finished_runs(&api.app.services).await.unwrap()
}

/// What the application wrote in the thread `thread` of Marc's direct
/// conversation, oldest first.
pub fn in_thread(slack: &FakeSlack, thread: &str) -> Vec<String> {
    in_thread_of(slack, MARC_SLACK, thread)
}

/// What the application wrote in the thread `thread` of the direct
/// conversation of the Slack user `person`, oldest first.
pub fn in_thread_of(slack: &FakeSlack, person: &str, thread: &str) -> Vec<String> {
    let channel = dm_of(person);
    slack
        .posts()
        .into_iter()
        .filter(|p| p.channel == channel && p.thread_ts.as_deref() == Some(thread))
        .map(|p| p.text)
        .collect()
}

/// The conversation held in the thread `thread`, if any.
pub async fn conversation_of(api: &TestApi, thread: &str) -> Option<Uuid> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let id = sqlx::query_scalar("select conversation_id from gateway_threads where thread_ts = $1")
        .bind(thread)
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    id
}

/// What became of the answers in the thread `thread`: their outcome (none
/// while to try again) and attempts, oldest first.
pub async fn replies_of(api: &TestApi, thread: &str) -> Vec<(Option<String>, i32)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_as(
        "select r.outcome, r.attempts from gateway_replies r \
         join gateway_threads t on t.id = r.thread_id where t.thread_ts = $1 \
         order by r.event_id",
    )
    .bind(thread)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}

/// Makes the answers in the thread `thread` waiting to be tried again due.
pub async fn retry_now(api: &TestApi, thread: &str) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update gateway_replies set retry_after = now() - interval '1 second' \
         where outcome is null and thread_id in \
         (select id from gateway_threads where thread_ts = $1)",
    )
    .bind(thread)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// The runs of the conversations held in the gateway's threads, newest
/// first (a gateway declared again goes with the former threads).
pub async fn slack_runs(api: &TestApi) -> Vec<(Uuid, String)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let runs = sqlx::query_as(
        "select r.id, r.status from agent_runs r \
         join gateway_threads t on t.conversation_id = r.conversation_id \
         where r.channel = 'slack' order by r.created_at desc",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    runs
}
