//! A company's own model providers: master keys, a key to store, the
//! factory that builds them, and a turn of conversation to see which
//! provider answers.

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use mimorya_agent::llm::{LlmProvider, ProviderConfig};
use mimorya_agent::testing::ScriptedProvider;
use mimorya_api::secrets::PUBLIC_TEST_MASTER_KEYS;
use mimorya_api::services::ProviderFactory;
use serde_json::json;

use super::Client;

/// Base64 of 32 bytes: the server's master key (public, refused in production).
pub const MASTER_KEY: &str = PUBLIC_TEST_MASTER_KEYS[0];
/// Another server's master key.
pub const OTHER_MASTER_KEY: &str = PUBLIC_TEST_MASTER_KEYS[1];
pub const API_KEY: &str = "sk-ant-api03-cle-de-test-0123456789";
pub const ENDPOINT: &str = "http://ollama.test:11434/v1";
/// The model a company chooses to review its answers (ADR-108): built as
/// `TestApi::review_model`.
pub const REVIEW_MODEL: &str = "modele-de-controle";

/// Builds a company's own providers, each configuration recorded in
/// `built`: `review` for `REVIEW_MODEL`, `company` for any other.
pub fn company_factory(
    company: Arc<ScriptedProvider>,
    review: Arc<ScriptedProvider>,
    built: Arc<Mutex<Vec<ProviderConfig>>>,
) -> ProviderFactory {
    Arc::new(move |config: &ProviderConfig| {
        built.lock().expect("built").push(config.clone());
        let provider: Arc<dyn LlmProvider> = if config.model.as_deref() == Some(REVIEW_MODEL) {
            review.clone()
        } else {
            company.clone()
        };
        Ok(provider)
    })
}

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
