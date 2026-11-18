//! A company's own model providers: master keys, a key to store, the
//! factory that builds them, and a turn of conversation to see which
//! provider answers. And the scripted model's prices, when a test counts
//! what its calls cost.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use mimorya_agent::llm::{LlmProvider, ModelPrice, PriceTable, ProviderConfig};
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

/// What a call of the scripted model costs when priced
/// (`TestApi::start_priced`): its 100 tokens in at 1500 USD a million, its
/// 20 out at 2500 USD a million.
pub const SCRIPTED_CALL_MICROS: i64 = 200_000;

/// The prices that make each call of the scripted model cost
/// `SCRIPTED_CALL_MICROS`, the escalation model the tests choose priced
/// alike (ADR-207: unpriced, it may not be chosen).
pub fn scripted_prices() -> PriceTable {
    let price = ModelPrice {
        input_per_mtok: 1500.0,
        output_per_mtok: 2500.0,
        cache_read_per_mtok: None,
        cache_write_per_mtok: None,
        long_prompt: None,
    };
    PriceTable::new(HashMap::from([
        ("scripted-model".to_string(), price),
        (ESCALATION_MODEL.to_string(), price),
    ]))
}

/// The escalation model the tests choose (ADR-207): priced, and served by
/// a scripted model of its own.
pub const ESCALATION_MODEL: &str = "claude-opus-4-8";

/// A company's own scripted models: its runs', its review model's and its
/// escalation model's.
pub struct CompanyModels {
    pub company: Arc<ScriptedProvider>,
    pub review: Arc<ScriptedProvider>,
    pub escalation: Arc<ScriptedProvider>,
}

/// Builds a company's own providers, each configuration recorded in
/// `built`: `review` for `REVIEW_MODEL`, `escalation` for
/// `ESCALATION_MODEL`, `company` for any other.
pub fn company_factory(
    models: &CompanyModels,
    built: Arc<Mutex<Vec<ProviderConfig>>>,
) -> ProviderFactory {
    let (company, review, escalation) = (
        models.company.clone(),
        models.review.clone(),
        models.escalation.clone(),
    );
    Arc::new(move |config: &ProviderConfig| {
        built.lock().expect("built").push(config.clone());
        let provider: Arc<dyn LlmProvider> = match config.model.as_deref() {
            Some(REVIEW_MODEL) => review.clone(),
            Some(ESCALATION_MODEL) => escalation.clone(),
            _ => company.clone(),
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
