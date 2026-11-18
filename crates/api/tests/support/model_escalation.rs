//! A company's model escalation as its tests choose it (ADR-207): its own
//! provider, an escalation of its model, and a choice refused.

use axum::http::StatusCode;
use serde_json::{Value, json};

use super::llm::{API_KEY, ENDPOINT, MASTER_KEY};
use super::{Client, TestApi};

pub const ESCALATION: &str = "/admin/model-escalation";
pub const MAIN: &str = "claude-haiku-5-5";
pub const STRONGER: &str = "claude-opus-5-5";

/// The app, able to keep a company's key, its endpoint listed.
pub async fn start() -> TestApi {
    TestApi::start_with(&[
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
        ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
    ])
    .await
}

/// Gives the company its own provider for its conversations.
pub async fn own_provider(paul: &Client<'_>, model: &str) {
    let body = json!({ "provider": "anthropic", "model": model, "apiKey": API_KEY });
    let chosen = paul.put("/admin/llm", body).await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text());
}

/// An escalation to `model`, no mechanism on, within 50 USD a month.
pub fn escalation(model: &str) -> Value {
    json!({
        "model": model,
        "afterFailure": false,
        "router": false,
        "advisor": false,
        "routes": { "conversation": "main", "mission": "main",
                    "scheduledTask": "main", "hook": "main" },
        "monthlyBudgetMicros": 50_000_000,
        "maxPerRun": 5,
    })
}

/// `body` refused, saying `said`.
pub async fn refused(paul: &Client<'_>, body: Value, said: &str) {
    let refused = paul.put(ESCALATION, body.clone()).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        refused.error_message().contains(said),
        "{body}: {}",
        refused.text()
    );
}
