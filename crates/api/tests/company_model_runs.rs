//! Which provider answers (ADR-024): the company's own, with its key, once
//! configured; the platform's otherwise; never the platform's when the
//! company's own cannot be used.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::llm::{API_KEY, MASTER_KEY, OTHER_MASTER_KEY, ask};

fn company_choice() -> Value {
    json!({ "provider": "anthropic", "model": "claude-sonnet-5-5", "apiKey": API_KEY })
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_provider_answers_with_its_own_key() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let platform = api.model.clone().unwrap();

    platform.push([ScriptedStep::text("Réponse de la plateforme.")]);
    let (status, answer) = ask(&api, &marc, "Bonjour").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(answer.contains("Réponse de la plateforme."));

    assert_eq!(
        paul.put("/admin/llm", company_choice()).await.status,
        StatusCode::OK
    );
    api.company_model
        .push([ScriptedStep::text("Réponse du modèle de l’entreprise.")]);
    let (status, answer) = ask(&api, &marc, "Bonjour").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(
        answer.contains("Réponse du modèle de l’entreprise."),
        "{answer}"
    );
    assert_eq!(platform.requests().len(), 1);
    let built = api.built.lock().unwrap().last().cloned().unwrap();
    assert_eq!(built.kind, "anthropic");
    assert_eq!(built.model.as_deref(), Some("claude-sonnet-5-5"));
    assert_eq!(built.api_key.as_deref(), Some(API_KEY));
    assert_eq!(built.base_url, None);

    // The connection test goes to the same provider, and is counted.
    api.company_model.push([ScriptedStep::text("OK")]);
    let tested = paul.post("/admin/llm/test", json!({})).await;
    assert_eq!(tested.status, StatusCode::OK);
    assert_eq!(tested.json()["ok"], true);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let counted: i64 = sqlx::query_scalar(
        "select count(*) from usage_records where run_id is null and user_id = $1",
    )
    .bind(user::PAUL)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(counted, 1);
    drop(tx);

    // Back to the platform.
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
    platform.push([ScriptedStep::text("De nouveau la plateforme.")]);
    let (_, answer) = ask(&api, &marc, "Bonjour").await;
    assert!(answer.contains("De nouveau la plateforme."));
    assert_eq!(api.company_model.remaining(), 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_unusable_company_provider_never_falls_back_to_the_platform() {
    let configured = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let paul = configured.sign_in(user::PAUL).await;
    assert_eq!(
        paul.put("/admin/llm", company_choice()).await.status,
        StatusCode::OK
    );

    // Another server, another master key: the stored key does not open.
    let other = TestApi::start_with(&[("MIMORYA_MASTER_KEY", OTHER_MASTER_KEY)]).await;
    let marc = other.sign_in(user::MARC).await;
    let me = marc.get("/auth/me").await.json();
    assert_eq!(me["assistant"]["available"], false);
    let (status, answer) = ask(&other, &marc, "Bonjour").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(answer.is_empty());
    assert!(other.model.as_ref().unwrap().requests().is_empty());
    let admin = other.sign_in(user::PAUL).await;
    assert_eq!(
        admin.get("/admin/llm").await.json()["settings"]["usable"],
        false
    );
    assert_eq!(
        admin.post("/admin/llm/test", json!({})).await.status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    // The configuration can be replaced or removed from there.
    assert_eq!(admin.delete("/admin/llm").await.status, StatusCode::OK);
    assert_eq!(
        marc.get("/auth/me").await.json()["assistant"]["available"],
        true
    );
}
