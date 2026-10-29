//! A conversation held in Telegram, as the application shows it (ADR-135):
//! listed and opened with its service, each message written to the bot
//! marked; what is written in the application follows it, answered in the
//! application only. Once its person asked the bot for a new one, or
//! archived it, it is held there no more, its messages still marked as
//! written there.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::FakeTelegram;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::slack_conversations::{answer, settle};
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{MARC_TELEGRAM, MARC_USERNAME};
use support::telegram_conversations::{chat_of, linked_bot, open_conversation, talk};

const MARC: (i64, &str) = (MARC_TELEGRAM, MARC_USERNAME);

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_conversation_held_in_telegram_shows_it_until_left_or_archived() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    api.script(vec![
        ScriptedStep::text("Réponse A."),
        ScriptedStep::text("Réponse B."),
        ScriptedStep::text("Réponse C."),
    ]);
    talk(&api, &telegram, id, 10, MARC, "Question A").await;
    assert_eq!(answer(&api).await, 1);
    let conversation = open_conversation(&api, id, MARC_TELEGRAM, user::MARC)
        .await
        .expect("its conversation");
    let path = format!("/conversations/{conversation}");

    // Listed and opened with its service, his message marked.
    let listed = marc.get("/conversations").await.json();
    let row = (listed["conversations"].as_array().unwrap().iter())
        .find(|c| c["id"] == conversation.to_string())
        .cloned()
        .unwrap();
    assert_eq!(row["gateway"], "telegram");
    let detail = marc.get(&path).await.json();
    assert_eq!(detail["conversation"]["gateway"], "telegram");
    assert_eq!(detail["messages"][0]["metadata"]["gateway"], "telegram");

    // Written in the application: it follows the conversation, answered in
    // the application only.
    let written = chat_of(&telegram, MARC_TELEGRAM).len();
    let sent = marc
        .post(
            &format!("{path}/messages"),
            json!({ "content": "Question B" }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    settle(&api).await;
    assert_eq!(answer(&api).await, 0);
    assert_eq!(chat_of(&telegram, MARC_TELEGRAM).len(), written);
    let detail = marc.get(&path).await.json();
    let messages = detail["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[2]["metadata"]["gateway"], Value::Null);
    assert_eq!(messages[3]["content"], "Réponse B.");

    // A new one asked for in Telegram: held there no more, his message
    // still marked as written there.
    talk(&api, &telegram, id, 11, MARC, "/nouvelle").await;
    let detail = marc.get(&path).await.json();
    assert_eq!(detail["conversation"]["gateway"], Value::Null);
    assert_eq!(detail["messages"][0]["metadata"]["gateway"], "telegram");
    let listed = marc.get("/conversations").await.json();
    assert!(
        (listed["conversations"].as_array().unwrap().iter())
            .all(|c| c["id"] != conversation.to_string() || c["gateway"].is_null()),
        "{listed}"
    );

    // Archived in the application: held there no more, until restored
    // before any message to the bot closes it.
    talk(&api, &telegram, id, 12, MARC, "Question C").await;
    let next = open_conversation(&api, id, MARC_TELEGRAM, user::MARC)
        .await
        .expect("the next conversation");
    let next_path = format!("/conversations/{next}");
    let gateway = |detail: Value| detail["conversation"]["gateway"].clone();
    let archived = marc.patch(&next_path, json!({ "archived": true })).await;
    assert_eq!(archived.status, StatusCode::OK, "{}", archived.text());
    assert_eq!(gateway(marc.get(&next_path).await.json()), Value::Null);
    let restored = marc.patch(&next_path, json!({ "archived": false })).await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
    assert_eq!(gateway(marc.get(&next_path).await.json()), "telegram");
}
