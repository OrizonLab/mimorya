//! A company's own model provider: master keys, a key to store, and a turn
//! of conversation to see which provider answers.

use axum::http::StatusCode;
use mimorya_api::secrets::PUBLIC_TEST_MASTER_KEYS;
use serde_json::json;

use super::Client;

/// Base64 of 32 bytes: the server's master key (public, refused in production).
pub const MASTER_KEY: &str = PUBLIC_TEST_MASTER_KEYS[0];
/// Another server's master key.
pub const OTHER_MASTER_KEY: &str = PUBLIC_TEST_MASTER_KEYS[1];
pub const API_KEY: &str = "sk-ant-api03-cle-de-test-0123456789";
pub const ENDPOINT: &str = "http://ollama.test:11434/v1";

/// Sends one message in a new conversation; returns the response status and
/// the assistant's answer once the run is over.
pub async fn ask(api: &super::TestApi, user: &Client<'_>, text: &str) -> (StatusCode, String) {
    let conversation = user.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sent = user
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": text }),
        )
        .await;
    api.idle().await;
    let thread = user.get(&format!("/conversations/{id}")).await.json();
    let answer = thread["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "assistant")
        .filter_map(|m| m["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    (sent.status, answer)
}
