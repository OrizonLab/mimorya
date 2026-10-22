//! The company's embedding model (ADR-074): chosen by a company-wide
//! settings administrator, its key sealed and never shown nor audited, its
//! endpoint fixed or listed by the operator, a key never carried from one
//! endpoint to another; tested at a cost recorded; dropped with the vectors.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::embedding::MISTRAL_BASE_URL;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_partial_admin};
use support::embeddings::{ENDPOINT, MODEL, choose, embedding_usage, start};
use support::llm::API_KEY;

async fn audited(api: &TestApi, action: &str) -> Value {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select coalesce(metadata, '{}'::jsonb) from audit_log where action = $1 \
         order by id desc limit 1",
    )
    .bind(action)
    .fetch_one(&mut *tx)
    .await
    .unwrap()
}

async fn sealed_keys(api: &TestApi) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar("select count(*) from secrets where name = 'embedding.api_key'")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_key_is_sealed_never_shown_and_dropped_with_the_choice() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let empty = paul.get("/admin/embedding").await.json();
    assert_eq!(empty["settings"], Value::Null);
    assert_eq!(empty["secretsAvailable"], true);
    assert_eq!(empty["endpoints"], json!([ENDPOINT]));
    let mistral = &empty["providers"][0];
    assert_eq!(mistral["kind"], "mistral");
    assert_eq!(mistral["defaultModel"], "mistral-embed");
    assert_eq!(mistral["requiresKey"], true);

    let chosen = json!({ "provider": "mistral", "apiKey": API_KEY });
    let saved = paul.put("/admin/embedding", chosen).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let shown = paul.get("/admin/embedding").await;
    assert!(!shown.text().contains(API_KEY));
    let settings = &shown.json()["settings"];
    assert_eq!(settings["model"], "mistral-embed");
    assert_eq!(settings["minSimilarity"], json!(0.75));
    assert_eq!(settings["keyStored"], true);
    assert_eq!(settings["usable"], true);
    assert_eq!(settings["embedded"], 0);
    assert!(settings["memories"].as_i64().unwrap() > 0);
    let configured = audited(&api, "embedding.configured").await;
    assert_eq!(configured["keyReplaced"], true);
    assert_eq!(configured["provider"], "mistral");
    assert!(!configured.to_string().contains(API_KEY));
    // Built with its key, at the official address only.
    api.app.services.embeddings.index_pending().await.unwrap();
    let built = api.embedders_built.lock().unwrap().last().cloned().unwrap();
    assert_eq!(built.base_url, MISTRAL_BASE_URL);
    assert_eq!(built.api_key.as_deref(), Some(API_KEY));

    assert_eq!(paul.delete("/admin/embedding").await.status, StatusCode::OK);
    assert_eq!(sealed_keys(&api).await, 0);
    let reset = audited(&api, "embedding.reset").await;
    assert!(reset["vectorsCleared"].as_u64().unwrap() > 0, "{reset}");
    assert_eq!(
        paul.get("/admin/embedding").await.json()["settings"],
        Value::Null
    );
    let again = paul.delete("/admin/embedding").await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_choice_follows_the_rules_and_keys_stay_with_their_endpoint() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    for body in [
        json!({ "provider": "anthropic", "apiKey": API_KEY }),
        json!({ "provider": "mistral" }),
        json!({ "provider": "mistral", "baseUrl": ENDPOINT, "apiKey": API_KEY }),
        json!({ "provider": "ollama", "model": "bge-m3", "baseUrl": "http://169.254.169.254/v1" }),
        json!({ "provider": "ollama", "baseUrl": ENDPOINT }),
        json!({ "provider": "ollama", "model": "bge-m3", "baseUrl": ENDPOINT, "minSimilarity": 1.0 }),
        json!({ "provider": "ollama", "model": "bge-m3", "baseUrl": ENDPOINT, "minSimilarity": 0 }),
        json!({ "provider": "openai", "apiKey": "avec des espaces" }),
        json!({ "provider": "openai", "apiKey": API_KEY, "dimensions": 1024 }),
    ] {
        let refused = paul.put("/admin/embedding", body.clone()).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{body}: {}",
            refused.text()
        );
    }
    assert_eq!(sealed_keys(&api).await, 0);

    let openai = json!({ "provider": "openai", "apiKey": API_KEY });
    assert_eq!(
        paul.put("/admin/embedding", openai).await.status,
        StatusCode::OK
    );
    // Same endpoint, other model: the key stays.
    let other = json!({ "provider": "openai", "model": "text-embedding-3-large" });
    assert_eq!(
        paul.put("/admin/embedding", other).await.status,
        StatusCode::OK
    );
    assert_eq!(sealed_keys(&api).await, 1);
    // Another provider: its key must be given, none follows.
    let refused = paul
        .put("/admin/embedding", json!({ "provider": "mistral" }))
        .await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.error_message().contains("clé"));
    // A listed server: the key is dropped, not carried there.
    choose(&paul, MODEL, 0.4).await;
    assert_eq!(sealed_keys(&api).await, 0);
    let shown = paul.get("/admin/embedding").await.json();
    assert_eq!(shown["settings"]["keyStored"], false);
    assert_eq!(shown["settings"]["minSimilarity"], json!(0.4));
    assert_eq!(paul.delete("/admin/embedding").await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_settings_administrators_choose_and_test() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let local = json!({ "provider": "ollama", "model": MODEL, "baseUrl": ENDPOINT });
    assert_eq!(
        marc.get("/admin/embedding").await.status,
        StatusCode::FORBIDDEN
    );
    let refused = marc.put("/admin/embedding", local.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.embeddings").await;
    let partial = activate(&api, &mut partial).await;
    let refused = partial.put("/admin/embedding", local).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let test = partial.post("/admin/embedding/test", json!({})).await;
    assert_eq!(test.status, StatusCode::FORBIDDEN);
    assert_eq!(
        partial.delete("/admin/embedding").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        paul.get("/admin/embedding").await.json()["settings"],
        Value::Null
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_test_embeds_a_sentence_at_a_recorded_cost() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let none = paul.post("/admin/embedding/test", json!({})).await;
    assert_eq!(none.status, StatusCode::SERVICE_UNAVAILABLE);

    choose(&paul, MODEL, 0.4).await;
    let tested = paul.post("/admin/embedding/test", json!({})).await.json();
    assert_eq!(
        tested,
        json!({ "ok": true, "provider": "test", "model": MODEL, "error": null })
    );
    let usage = embedding_usage(&api).await;
    assert_eq!(usage[0]["userId"], json!(user::PAUL));
    assert_eq!(usage[0]["runId"], json!(null));
    assert_eq!(audited(&api, "embedding.tested").await["ok"], true);

    api.embedder.fail(true);
    let failed = paul.post("/admin/embedding/test", json!({})).await.json();
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["error"], "Fournisseur indisponible");
    assert_eq!(audited(&api, "embedding.tested").await["ok"], false);
    assert_eq!(embedding_usage(&api).await.len(), usage.len());
    api.embedder.fail(false);
    assert_eq!(paul.delete("/admin/embedding").await.status, StatusCode::OK);
}
