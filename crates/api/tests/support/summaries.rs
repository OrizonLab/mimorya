//! Conversations' summaries (ADR-145): exchanges written straight into a
//! conversation (as earlier turns would have left them), a turn of its
//! owner, the summary stored and what the model was given.

use axum::http::StatusCode;
use mimorya_agent::llm::LlmMessage;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call};
use serde_json::json;
use uuid::Uuid;

use super::{Client, TestApi, owner_url, week};

/// What the model answers when asked for the summary.
pub const SUMMARY: &str = "Le collaborateur prépare la semaine 42 : INT-2049 reste en cours.";
/// The system prompt of the request asking for it.
pub const SUMMARY_SYSTEM: &str = "Tu tiens le résumé d’une conversation";

/// A new conversation of `client`'s.
pub async fn conversation(client: &Client<'_>) -> Uuid {
    let opened = client.post("/conversations", json!({})).await.json();
    opened["conversation"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// `count` exchanges added to `conversation`, numbered from `from`
/// (« Question n » then « Réponse n »).
pub async fn exchanges(conversation: Uuid, from: usize, count: usize) {
    padded_exchanges(conversation, from, count, 0).await;
}

/// The same, each message followed by `padding` characters.
pub async fn padded_exchanges(conversation: Uuid, from: usize, count: usize, padding: usize) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let pad = if padding == 0 {
        String::new()
    } else {
        format!(" {}", "x".repeat(padding))
    };
    for n in from..from + count {
        for (role, text) in [("user", "Question"), ("assistant", "Réponse")] {
            sqlx::query(
                "insert into messages (tenant_id, conversation_id, role, content, created_at) \
                 select tenant_id, id, $2, $3, clock_timestamp() from conversations \
                 where id = $1",
            )
            .bind(conversation)
            .bind(role)
            .bind(format!("{text} {n}{pad}"))
            .execute(&owner)
            .await
            .unwrap();
        }
    }
    owner.close().await;
}

/// A turn of `client` in `conversation` once `steps` are scripted, then
/// whatever runs in the background; its run.
pub async fn turn(
    api: &TestApi,
    client: &Client<'_>,
    conversation: Uuid,
    text: &str,
    steps: Vec<ScriptedStep>,
) -> Uuid {
    api.script(steps);
    let sent = client
        .post(
            &format!("/conversations/{conversation}/messages"),
            json!({ "content": text }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    api.idle().await;
    sent.json()["runId"].as_str().unwrap().parse().unwrap()
}

/// The conversation's summary as stored: its text, the message it stops
/// at, and the messages folded in so far.
pub async fn stored(conversation: Uuid) -> Option<(String, String, i32)> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let row = sqlx::query_as(
        "select s.content, m.content, s.folded_messages from conversation_summaries s \
         join messages m on m.id = s.through_message_id where s.conversation_id = $1",
    )
    .bind(conversation)
    .fetch_optional(&owner)
    .await
    .unwrap();
    owner.close().await;
    row
}

/// The texts of the messages the model was given last, in order.
pub fn given(api: &TestApi) -> Vec<String> {
    let requests = api.model.as_ref().unwrap().requests();
    requests
        .last()
        .unwrap()
        .messages
        .iter()
        .map(|m| match m {
            LlmMessage::User { content } => content.clone(),
            LlmMessage::Assistant { content, .. } => content.clone(),
            LlmMessage::ToolResults { .. } => String::from("[résultats]"),
        })
        .collect()
}

/// The system prompt of the last request to the model.
pub fn last_system(api: &TestApi) -> String {
    let requests = api.model.as_ref().unwrap().requests();
    requests.last().unwrap().system.clone()
}

/// `account`'s status set, as an administrator would.
pub async fn set_status(account: Uuid, status: &str) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("update users set status = $2 where id = $1")
        .bind(account)
        .bind(status)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

/// A step during which `account` is disabled, while the model answers,
/// then asks for a search.
pub fn disabling(account: Uuid) -> ScriptedStep {
    ScriptedStep::dynamic(move |_| {
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("runtime")
                .block_on(set_status(account, "disabled"));
        })
        .join()
        .expect("change thread");
        ScriptedReply::calls("Je cherche.", vec![call("interventions_search", week())])
    })
}

/// How many passes were audited for `conversation`.
pub async fn audited_passes(conversation: Uuid) -> i64 {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let count = sqlx::query_scalar(
        "select count(*) from audit_log where action = 'conversation.summarised' \
         and resource_id = $1",
    )
    .bind(conversation.to_string())
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    count
}
