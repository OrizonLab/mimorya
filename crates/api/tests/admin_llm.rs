//! The company's own model provider (ADR-024): chosen by a company-wide
//! settings administrator, its key sealed and never shown nor audited, its
//! endpoint fixed or listed by the operator, and a key never carried from one
//! endpoint to another.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_partial_admin};
use support::llm::{API_KEY, ENDPOINT, MASTER_KEY};

fn anthropic(key: Option<&str>) -> Value {
    let mut body = json!({ "provider": "anthropic", "model": "claude-opus-5-5" });
    if let Some(key) = key {
        body["apiKey"] = json!(key);
    }
    body
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_key_is_sealed_and_never_shown() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let paul = api.sign_in(user::PAUL).await;
    let empty = paul.get("/admin/llm").await.json();
    assert_eq!(empty["secretsAvailable"], true);
    assert_eq!(empty["settings"], Value::Null);

    let saved = paul.put("/admin/llm", anthropic(Some(API_KEY))).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let shown = paul.get("/admin/llm").await;
    assert!(!shown.text().contains(API_KEY));
    let settings = &shown.json()["settings"];
    assert_eq!(settings["provider"], "anthropic");
    assert_eq!(settings["keyStored"], true);
    assert_eq!(settings["usable"], true);
    assert_eq!(settings["priced"], true);

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let sealed: Vec<u8> =
        sqlx::query_scalar("select ciphertext from secrets where name = 'llm.api_key'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(
        !sealed
            .windows(API_KEY.len())
            .any(|w| w == API_KEY.as_bytes())
    );
    let audited: Value = sqlx::query_scalar(
        "select metadata from audit_log where action = 'llm.configured' order by id desc limit 1",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(audited["keyReplaced"], true);
    assert!(!audited.to_string().contains(API_KEY));
    drop(tx);

    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
    assert_eq!(paul.get("/admin/llm").await.json()["settings"], Value::Null);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let left: i64 = sqlx::query_scalar("select count(*) from secrets where name = 'llm.api_key'")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(left, 0);
    assert_eq!(
        paul.delete("/admin/llm").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_choice_follows_the_rules_and_keys_stay_with_their_endpoint() {
    let api = TestApi::start_with(&[
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
        ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
    ])
    .await;
    let paul = api.sign_in(user::PAUL).await;
    for body in [
        json!({ "provider": "mistral", "model": "m", "apiKey": API_KEY }),
        json!({ "provider": "openai", "apiKey": API_KEY }),
        json!({ "provider": "anthropic", "baseUrl": "http://169.254.169.254", "apiKey": API_KEY }),
        json!({ "provider": "ollama", "model": "llama3", "baseUrl": "http://localhost:5432" }),
        json!({ "provider": "openai_compatible", "model": "m" }),
        json!({ "provider": "anthropic", "apiKey": "avec des espaces" }),
        json!({ "provider": "anthropic", "apiKey": API_KEY, "region": "eu" }),
        anthropic(None),
    ] {
        let refused = paul.put("/admin/llm", body.clone()).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{body}: {}",
            refused.text()
        );
    }

    let local = json!({ "provider": "ollama", "model": "llama3.3", "baseUrl": ENDPOINT });
    assert_eq!(
        paul.put("/admin/llm", local.clone()).await.status,
        StatusCode::OK
    );
    // Another provider: its key must be given, none follows.
    let without_key = json!({ "provider": "openrouter", "model": "anthropic/claude-opus-5-5" });
    let refused = paul.put("/admin/llm", without_key.clone()).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.error_message().contains("clé"));
    let mut with_key = without_key.clone();
    with_key["apiKey"] = json!(API_KEY);
    assert_eq!(
        paul.put("/admin/llm", with_key).await.status,
        StatusCode::OK
    );
    // Same endpoint, other model: the key stays.
    let other_model = json!({ "provider": "openrouter", "model": "openai/gpt-5" });
    assert_eq!(
        paul.put("/admin/llm", other_model).await.status,
        StatusCode::OK
    );
    let shown = paul.get("/admin/llm").await.json();
    assert_eq!(shown["settings"]["keyStored"], true);
    assert_eq!(shown["settings"]["priced"], false);
    // Back to the local server: the key is dropped, not carried there.
    assert_eq!(paul.put("/admin/llm", local).await.status, StatusCode::OK);
    let shown = paul.get("/admin/llm").await.json();
    assert_eq!(shown["settings"]["keyStored"], false);
    assert_eq!(shown["endpoints"], json!([ENDPOINT]));
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);

    // Company-wide settings administrators only.
    let marc = api.sign_in(user::MARC).await;
    assert_eq!(marc.get("/admin/llm").await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        marc.put("/admin/llm", anthropic(Some(API_KEY)))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.modele").await;
    let partial = activate(&api, &mut partial).await;
    assert_eq!(
        partial
            .put("/admin/llm", anthropic(Some(API_KEY)))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        partial.post("/admin/llm/test", json!({})).await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_master_key_no_key_can_be_stored() {
    let api = TestApi::start_with(&[("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT)]).await;
    let paul = api.sign_in(user::PAUL).await;
    assert_eq!(
        paul.get("/admin/llm").await.json()["secretsAvailable"],
        false
    );
    let refused = paul.put("/admin/llm", anthropic(Some(API_KEY))).await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(refused.error_message().contains("MIMORYA_MASTER_KEY"));
    // A local server without key needs no secret.
    let local = json!({ "provider": "ollama", "model": "llama3.3", "baseUrl": ENDPOINT });
    assert_eq!(paul.put("/admin/llm", local).await.status, StatusCode::OK);
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}
