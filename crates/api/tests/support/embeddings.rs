//! A company's embedding model (ADR-074): the concepts the test model
//! knows, choosing it through the administration, the vectors stored, and
//! what a turn recalls.

use axum::http::StatusCode;
use mimorya_agent::llm::LlmMessage;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::embeddings::Indexed;
use mimorya_core::demo::tenant;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, TestApi};

/// Words of one concept: the test model brings them together.
pub const CONCEPTS: &[&[&str]] = &[
    &["véhicule", "camionnette", "fourgon", "utilitaire"],
    &["astreinte", "permanence"],
];
/// An endpoint the operator lists (`MIMORYA_TENANT_LLM_ENDPOINTS`).
pub const ENDPOINT: &str = "http://embeddings.test:8080/v1";
pub const MODEL: &str = "concepts";
/// A memory found by meaning only from `ASKED`: no word in common.
pub const VANS: (&str, &str) = (
    "Départ des tournées",
    "Les camionnettes partent du dépôt de Lyon à 7 h.",
);
pub const ASKED: &str = "Quel véhicule prendre demain ?";

const ENV: [(&str, &str); 2] = [
    ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
    ("MIMORYA_MASTER_KEY", super::llm::MASTER_KEY),
];

/// The app with `ENDPOINT` listed and the server's master key; memories
/// indexed on demand (`index_pending`).
pub async fn start() -> TestApi {
    TestApi::start_with(&ENV).await
}

/// The same, memories indexed in the background.
pub async fn start_indexing() -> TestApi {
    TestApi::start_indexing(&ENV).await
}

/// `start`, in the role `role` (ADR-085).
pub async fn start_as(role: &str) -> TestApi {
    TestApi::start_with(&[ENV[0], ENV[1], ("MIMORYA_ROLE", role)]).await
}

/// `start_indexing`, in the role `role`.
pub async fn start_indexing_as(role: &str) -> TestApi {
    TestApi::start_indexing(&[ENV[0], ENV[1], ("MIMORYA_ROLE", role)]).await
}

/// Chooses the test model at `ENDPOINT`, the vectors close from
/// `min_similarity`.
pub async fn choose(admin: &Client<'_>, model: &str, min_similarity: f32) {
    let body = json!({
        "provider": "openai_compatible",
        "model": model,
        "baseUrl": ENDPOINT,
        "minSimilarity": min_similarity,
    });
    let chosen = admin.put("/admin/embedding", body).await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text());
}

/// A pass of the indexer, as the background would run it: how many
/// memories got a vector (the demo documents' passages get theirs too).
pub async fn index(api: &TestApi) -> usize {
    indexed(api).await.memories
}

/// A pass of the indexer: how many memories and passages got a vector.
pub async fn indexed(api: &TestApi) -> Indexed {
    api.app.services.embeddings.index_pending().await.unwrap()
}

pub async fn forget(client: &Client<'_>, memory: Uuid) {
    let deleted = client.delete(&format!("/memory/{memory}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}

/// Back to searching by words only: the vectors are wiped.
pub async fn back_to_words(admin: &Client<'_>) {
    let reset = admin.delete("/admin/embedding").await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
}

/// Writes a personal memory of `client`; its identifier.
pub async fn remember(client: &Client<'_>, title: &str, content: &str) -> Uuid {
    let body = json!({ "space": "personal", "kind": "fact", "title": title, "content": content });
    let created = client.post("/memory", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().parse().unwrap()
}

/// The model a memory's vector comes from, and whether it has one.
pub async fn vector_of(api: &TestApi, memory: Uuid) -> (Option<String>, bool) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let row: (Option<String>, bool) = sqlx::query_as(
        "select embedding_model, embedding is not null from memory_items where id = $1",
    )
    .bind(memory)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    row
}

/// What a turn of `client` asking `text` handed the model: its user turn,
/// where the recalled memories travel.
pub async fn recalled(api: &TestApi, client: &Client<'_>, text: &str) -> String {
    api.script(vec![ScriptedStep::text("C’est noté.")]);
    let conversation = client.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let sent = client
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": text }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    api.idle().await;
    let requests = api.model.as_ref().unwrap().requests();
    match requests.last().and_then(|r| r.messages.last()) {
        Some(LlmMessage::User { content }) => content.clone(),
        other => panic!("a user turn last, not {other:?}"),
    }
}

/// The embedding usage recorded in the company, newest first.
pub async fn embedding_usage(api: &TestApi) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select json_build_object('model', model, 'provider', provider, 'userId', user_id, \
         'runId', run_id, 'inputTokens', input_tokens) \
         from usage_records where kind = 'embedding' order by created_at desc, id desc",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}
