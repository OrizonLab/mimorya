//! Runs started as a user would: a conversation, then a message to it.

use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

use super::Client;

/// Starts a run on behalf of `client` in a new conversation; returns its id.
pub async fn start_run(client: &Client<'_>) -> Uuid {
    let conversation = client.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let started = client
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": "Fais le point sur la semaine" }),
        )
        .await;
    assert_eq!(started.status, StatusCode::ACCEPTED, "{}", started.text());
    started.json()["runId"].as_str().unwrap().parse().unwrap()
}
