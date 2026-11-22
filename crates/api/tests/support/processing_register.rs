//! The record of processing activities as the tests read it (ADR-217): the
//! app keeping keys of its own, of the operator and of Jev, none of which
//! the register ever shows; its activities, their recipients and lines.

use axum::http::StatusCode;
use mimorya_connectors::testing::JEV_KEY;
use serde_json::Value;

use super::embeddings::ENDPOINT;
use super::llm::{API_KEY, MASTER_KEY, OPERATOR_KEY};
use super::{Client, TestApi};

pub const REGISTER: &str = "/admin/processing-register";

/// The app with a master key, an endpoint listed, and the operator's
/// model at Anthropic.
pub async fn start() -> TestApi {
    TestApi::start_with(&[
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
        ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
        ("LLM_PROVIDER", "anthropic"),
        ("ANTHROPIC_API_KEY", OPERATOR_KEY),
    ])
    .await
}

/// The register as `client` reads it, never holding a key.
pub async fn read(client: &Client<'_>) -> Value {
    let read = client.get(REGISTER).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    for secret in [API_KEY, OPERATOR_KEY, JEV_KEY] {
        assert!(!read.text().contains(secret));
    }
    read.json()
}

pub fn activity<'a>(register: &'a Value, key: &str) -> &'a Value {
    register["activities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|activity| activity["key"] == key)
        .unwrap_or_else(|| panic!("no activity {key}"))
}

pub fn keys(register: &Value) -> Vec<String> {
    register["activities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|activity| activity["key"].as_str().unwrap().to_string())
        .collect()
}

/// Each recipient of the activity `key`, as (name, place, chosen by).
pub fn recipients(register: &Value, key: &str) -> Vec<(String, String, String)> {
    activity(register, key)["recipients"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let text = |field: &str| r[field].as_str().unwrap().to_string();
            (text("name"), text("place"), text("chosenBy"))
        })
        .collect()
}

pub fn of(name: &str, place: &str, by: &str) -> (String, String, String) {
    (name.into(), place.into(), by.into())
}

pub fn lines(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap().to_string())
        .collect()
}
