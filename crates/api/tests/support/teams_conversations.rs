//! A person talking with the agent in Teams in the tests (ADR-140): their
//! Teams identity linked to their account from the bot's link, what they
//! write to the bot as the Bot Framework sends it, the conversations their
//! personal conversation with the bot holds, and the answers the pass
//! writes back there.

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeTeams, TEAMS_APP, TeamsSent, conversation_of};
use mimorya_core::demo::tenant;
use serde_json::json;
use uuid::Uuid;

use super::slack_conversations::settle;
use super::teams::{declare_teams, link_code, post_activity, write};
use super::{Client, TestApi, email};

/// The bot, declared by `paul`, the Teams person `from` (their Entra
/// object, their name) linked to `client`'s account, whose address is
/// `address`, from the bot's link.
pub async fn linked_teams(
    api: &TestApi,
    teams: &FakeTeams,
    (paul, client): (&Client<'_>, &Client<'_>),
    from: (&str, &str),
    user_id: Uuid,
) -> Uuid {
    let id = declare_teams(paul).await;
    teams.add_person(from.0, from.1, &email(user_id));
    let code = link_code(api, teams, id, "link-1", from).await;
    let linked = (client.post("/account/gateways/link", json!({ "code": code }))).await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());
    id
}

/// Sends what `from` writes to the bot `id` as the activity `activity_id`,
/// then waits for what it started.
pub async fn talk(
    api: &TestApi,
    teams: &FakeTeams,
    id: Uuid,
    activity_id: &str,
    from: (&str, &str),
    text: &str,
) {
    write(api, teams, id, activity_id, from, text).await;
    settle(api).await;
}

/// The same, written from the Teams service `service_url` (another region
/// of Microsoft's, say).
pub async fn talk_from(
    api: &TestApi,
    teams: &FakeTeams,
    (id, service_url): (Uuid, &str),
    activity_id: &str,
    from: (&str, &str),
    text: &str,
) {
    let activity = teams.message_from(service_url, (TEAMS_APP, activity_id), from, text);
    let token = format!("Bearer {}", teams.token_of(service_url, TEAMS_APP));
    let sent = post_activity(api, id, Some(&token), &activity).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    settle(api).await;
}

/// What the bots wrote in the personal conversation of `person`, oldest
/// first.
pub fn said_to(teams: &FakeTeams, person: &str) -> Vec<TeamsSent> {
    let conversation = conversation_of(person);
    (teams.sent().into_iter())
        .filter(|m| m.conversation == conversation)
        .collect()
}

/// What the bots last wrote to `person`.
pub fn last_said(teams: &FakeTeams, person: &str) -> String {
    said_to(teams, person).pop().expect("a message").text
}

/// The conversation of the account `user_id` open in the personal
/// conversation of `person` with the bot `id`, if any.
pub async fn open_teams_conversation(
    api: &TestApi,
    id: Uuid,
    person: &str,
    user_id: Uuid,
) -> Option<Uuid> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let open = sqlx::query_scalar(
        "select conversation_id from gateway_threads where gateway_id = $1 \
         and channel_id = $2 and user_id = $3 and closed_at is null",
    )
    .bind(id)
    .bind(conversation_of(person))
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    open
}

/// The Teams services the threads of the personal conversation of
/// `person` answer at, oldest first.
pub async fn teams_services(api: &TestApi, person: &str) -> Vec<String> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let services = sqlx::query_scalar(
        "select service_url from gateway_threads where channel_id = $1 order by created_at",
    )
    .bind(conversation_of(person))
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    services
}

/// The runs started by writing to the bot, newest first: their
/// conversation and status.
pub async fn teams_runs(api: &TestApi) -> Vec<(Uuid, Uuid, String)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let runs = sqlx::query_as(
        "select r.id, r.conversation_id, r.status from agent_runs r \
         join gateway_threads t on t.conversation_id = r.conversation_id \
         where r.channel = 'teams' order by r.created_at desc",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    runs
}

/// What became of the answers in the personal conversation of `person`:
/// their outcome (none while to try again) and attempts, oldest first.
pub async fn teams_replies(api: &TestApi, person: &str) -> Vec<(Option<String>, i32)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_as(
        "select r.outcome, r.attempts from gateway_replies r \
         join gateway_threads t on t.id = r.thread_id where t.channel_id = $1 \
         order by r.event_id",
    )
    .bind(conversation_of(person))
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}

/// The answers waiting in the personal conversation of `person`, tried
/// again now.
pub async fn retry_teams_now(api: &TestApi, person: &str) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update gateway_replies set retry_after = now() - interval '1 second' \
         where outcome is null and thread_id in \
         (select id from gateway_threads where channel_id = $1)",
    )
    .bind(conversation_of(person))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}
