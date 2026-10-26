//! Which provider answers (ADR-024): the company's own, with its key, once
//! configured; the platform's otherwise; never the platform's when the
//! company's own cannot be used.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::{LlmUsage, official_endpoint};
use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_api::services::llm::Resolution;
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::{Value, json};
use support::TestApi;
use support::llm::{API_KEY, ENDPOINT, MASTER_KEY, OTHER_MASTER_KEY, ask};

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
    // The official endpoint, written out: never the server's ANTHROPIC_BASE_URL.
    assert_eq!(built.base_url.as_deref(), official_endpoint("anthropic"));

    // The connection test goes to the same provider, and is counted (cached
    // input tokens once).
    let cached = LlmUsage {
        input_tokens: 40,
        output_tokens: 2,
        cache_read_tokens: 10,
        cache_write_tokens: 5,
    };
    api.company_model.push([ScriptedStep::Fixed(
        ScriptedReply::text("OK").with_usage(cached),
    )]);
    let tested = paul.post("/admin/llm/test", json!({})).await;
    assert_eq!(tested.status, StatusCode::OK);
    assert_eq!(tested.json()["ok"], true);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let counted: Vec<i32> = sqlx::query_scalar(
        "select input_tokens from usage_records where run_id is null and user_id = $1",
    )
    .bind(user::PAUL)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(counted, [55]);
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
    // A server without any master key says so.
    let keyless = TestApi::start().await;
    let shown = keyless
        .sign_in(user::PAUL)
        .await
        .get("/admin/llm")
        .await
        .json();
    assert!(
        shown["settings"]["problem"]
            .as_str()
            .unwrap()
            .contains("MIMORYA_MASTER_KEY"),
        "{shown}"
    );
    // The configuration can be replaced or removed from there.
    assert_eq!(admin.delete("/admin/llm").await.status, StatusCode::OK);
    assert_eq!(
        marc.get("/auth/me").await.json()["assistant"]["available"],
        true
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_provider_serves_its_company_only() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let llm = &api.app.services.llm;
    let paul = api.sign_in(user::PAUL).await;
    assert_eq!(
        paul.put("/admin/llm", company_choice()).await.status,
        StatusCode::OK
    );
    // Resolved (and cached) for the company first, then for another one.
    assert!(matches!(
        &llm.resolve(tenant::ID).await.unwrap().runs,
        Resolution::Company(_)
    ));
    assert!(matches!(
        &llm.resolve(OTHER_TENANT_ID).await.unwrap().runs,
        Resolution::Platform(Some(_))
    ));
    assert_eq!(api.built.lock().unwrap().len(), 1);
    // Its settings and key are not even visible from the other company.
    let mut other = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    for table in ["secrets", "tenant_llm_settings"] {
        let visible: i64 = sqlx::query_scalar(&format!("select count(*) from {table}"))
            .fetch_one(&mut *other)
            .await
            .unwrap();
        assert_eq!(visible, 0, "{table}");
    }
    drop(other);
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_endpoint_the_operator_removed_is_no_longer_reached() {
    let listed = TestApi::start_with(&[
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
        ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
    ])
    .await;
    let paul = listed.sign_in(user::PAUL).await;
    let local = json!({ "provider": "ollama", "model": "llama3.3", "baseUrl": ENDPOINT });
    assert_eq!(paul.put("/admin/llm", local).await.status, StatusCode::OK);

    // The operator restarts without that endpoint.
    let unlisted = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let marc = unlisted.sign_in(user::MARC).await;
    assert_eq!(
        marc.get("/auth/me").await.json()["assistant"]["available"],
        false
    );
    let (status, _) = ask(&unlisted, &marc, "Bonjour").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(unlisted.built.lock().unwrap().is_empty());
    assert!(unlisted.model.as_ref().unwrap().requests().is_empty());
    let admin = unlisted.sign_in(user::PAUL).await;
    let shown = admin.get("/admin/llm").await.json();
    assert_eq!(shown["settings"]["usable"], false);
    assert!(
        shown["settings"]["problem"]
            .as_str()
            .unwrap()
            .contains("adresse")
    );
    assert_eq!(admin.delete("/admin/llm").await.status, StatusCode::OK);
}
