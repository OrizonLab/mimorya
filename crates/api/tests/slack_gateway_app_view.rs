//! A conversation held in Slack as the application shows it (ADR-128):
//! listed and opened with the service it is held in, its author's messages
//! written there marked so; a message written in the application follows
//! it, answered in the application only; another person reaches nothing of
//! it.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::FakeSlack;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::slack_conversations::{
    MARC_SLACK, answer, conversation_of, in_thread, linked, settle, write, written,
};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, email};

const FIRST: &str = "1700000200.000100";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_conversation_held_in_slack_is_shown_as_such() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    api.script(vec![
        ScriptedStep::text("Réponse A."),
        ScriptedStep::text("Réponse B."),
    ]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question A", FIRST, None),
    )
    .await;
    assert_eq!(answer(&api).await, 1);
    let conversation = conversation_of(&api, FIRST)
        .await
        .expect("its conversation");
    let path = format!("/conversations/{conversation}");

    // Listed with its service; one opened in the application, with none.
    let opened = marc.post("/conversations", json!({})).await;
    assert_eq!(opened.status, StatusCode::CREATED, "{}", opened.text());
    assert_eq!(opened.json()["conversation"]["gateway"], Value::Null);
    let own = opened.json()["conversation"]["id"].clone();
    let listed = marc.get("/conversations").await.json();
    let rows = listed["conversations"].as_array().unwrap();
    let row = |id: &Value| rows.iter().find(|c| &c["id"] == id).cloned().unwrap();
    let held = row(&json!(conversation));
    assert_eq!(held["gateway"], "slack");
    assert_eq!(held["title"], "Question A");
    assert_eq!(row(&own)["gateway"], Value::Null);

    // Opened: its service, his message marked as written there.
    let detail = marc.get(&path).await.json();
    assert_eq!(detail["conversation"]["gateway"], "slack");
    assert_eq!(detail["messages"][0]["metadata"]["gateway"], "slack");
    assert_eq!(detail["messages"][1]["content"], "Réponse A.");

    // Written in the application: it follows the conversation, answered in
    // the application only.
    let sent = marc
        .post(
            &format!("{path}/messages"),
            json!({ "content": "Question B" }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    settle(&api).await;
    assert_eq!(answer(&api).await, 0);
    assert_eq!(in_thread(&slack, FIRST).len(), 1);
    let detail = marc.get(&path).await.json();
    let messages = detail["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[2]["content"], "Question B");
    assert_eq!(messages[2]["metadata"]["gateway"], Value::Null);
    assert_eq!(messages[3]["content"], "Réponse B.");

    // Another person reaches nothing of it.
    let lea = api.sign_in(user::LEA).await;
    assert_eq!(lea.get(&path).await.status, StatusCode::NOT_FOUND);
    let theirs = lea.get("/conversations").await.json();
    assert!(!theirs.to_string().contains(&conversation.to_string()));
}
