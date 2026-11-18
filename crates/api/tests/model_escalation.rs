//! The company's model escalation (ADR-207): a model of its conversations'
//! provider, chosen by an administrator of its settings on the whole
//! company only, within a monthly budget and a number of escalations per
//! run; audited, removed. A company on the platform's model may not choose
//! one, and one chosen stops serving once the conversations' provider
//! changes. What its mechanisms cost this month is read from the usage by
//! role, shown with the usage, and kept to the company: another neither
//! reads nor writes it.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_core::demo::{tenant, user};
use mimorya_core::time_zone::month_start;
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::{Value, json};
use support::accounts::{activate, create_partial_admin};
use support::llm::{API_KEY, ENDPOINT, MASTER_KEY};
use support::refusals::{entries_since, latest_entry};
use support::{Client, TestApi};

const ESCALATION: &str = "/admin/model-escalation";
const MAIN: &str = "claude-haiku-5-5";
const STRONGER: &str = "claude-opus-5-5";

async fn start() -> TestApi {
    TestApi::start_with(&[
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
        ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
    ])
    .await
}

/// Gives the company its own provider for its conversations.
async fn own_provider(paul: &Client<'_>, model: &str) {
    let body = json!({ "provider": "anthropic", "model": model, "apiKey": API_KEY });
    let chosen = paul.put("/admin/llm", body).await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text());
}

fn escalation(model: &str) -> Value {
    json!({
        "model": model,
        "afterFailure": false,
        "router": false,
        "advisor": false,
        "monthlyBudgetMicros": 50_000_000,
        "maxPerRun": 5,
    })
}

async fn refused(paul: &Client<'_>, body: Value, said: &str) {
    let refused = paul.put(ESCALATION, body.clone()).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        refused.error_message().contains(said),
        "{body}: {}",
        refused.text()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_chooses_its_escalation_model_within_its_bounds() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    own_provider(&paul, MAIN).await;
    let since = latest_entry(&api).await;
    let empty = paul.get(ESCALATION).await.json();
    assert_eq!(empty["mainModel"], MAIN);
    assert_eq!(empty["settings"], Value::Null);
    assert!(empty["spentMicros"].is_i64(), "{empty}");
    assert_eq!(empty["minBudgetMicros"], 1_000_000);
    assert_eq!(empty["maxBudgetMicros"], 10_000_000_000_i64);
    assert_eq!(empty["maxPerRunCeiling"], 50);
    let mechanisms = empty["mechanisms"].as_array().unwrap();
    assert_eq!(mechanisms.len(), 3);
    assert!(
        mechanisms
            .iter()
            .all(|m| m["available"] == false && m["reason"] == "Bientôt disponible"),
        "{mechanisms:?}"
    );

    let mut cheap = escalation(STRONGER);
    cheap["monthlyBudgetMicros"] = json!(999_999);
    for (body, said) in [
        (escalation(MAIN), "doit différer du modèle principal"),
        (escalation(""), "Indiquez le modèle d’escalade"),
        (escalation("claude opus"), "Nom de modèle invalide"),
        (cheap, "1 à 10 000 USD"),
    ] {
        refused(&paul, body, said).await;
    }
    let mut too_many = escalation(STRONGER);
    too_many["maxPerRun"] = json!(51);
    refused(&paul, too_many, "1 à 50").await;
    let mut early = escalation(STRONGER);
    early["afterFailure"] = json!(true);
    refused(&paul, early, "pas encore disponible").await;
    let mut unknown = escalation(STRONGER);
    unknown["provider"] = json!("openai");
    let unknown = paul.put(ESCALATION, unknown).await;
    assert_eq!(
        unknown.status,
        StatusCode::BAD_REQUEST,
        "{}",
        unknown.text()
    );

    let chosen = paul.put(ESCALATION, escalation(STRONGER)).await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text());
    let shown = paul.get(ESCALATION).await.json();
    let settings = &shown["settings"];
    assert_eq!(settings["model"], STRONGER);
    assert_eq!(settings["monthlyBudgetMicros"], 50_000_000);
    assert_eq!(settings["maxPerRun"], 5);
    assert_eq!(settings["afterFailure"], false);
    assert_eq!(shown["problem"], Value::Null);
    let mut cheaper = escalation("claude-sonnet-5-5");
    cheaper["maxPerRun"] = json!(50);
    assert_eq!(paul.put(ESCALATION, cheaper).await.status, StatusCode::OK);
    assert_eq!(
        paul.get(ESCALATION).await.json()["settings"]["maxPerRun"],
        50
    );

    assert_eq!(paul.delete(ESCALATION).await.status, StatusCode::OK);
    assert_eq!(paul.get(ESCALATION).await.json()["settings"], Value::Null);
    let again = paul.delete(ESCALATION).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
    assert!(
        again.error_message().contains("Aucune escalade"),
        "{}",
        again.text()
    );
    let audit = entries_since(&api, since).await;
    for said in [
        "model_escalation.configured",
        "\"model\":\"claude-opus-5-5\"",
        "\"previousModel\":\"claude-opus-5-5\"",
        "\"monthlyBudgetMicros\":50000000",
        "model_escalation.reset",
    ] {
        assert!(audit.contains(said), "{said}: {audit}");
    }
    assert!(!audit.contains(API_KEY), "{audit}");
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_on_the_platform_s_model_may_not_choose_one() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let shown = paul.get(ESCALATION).await.json();
    assert_eq!(shown["mainModel"], Value::Null);
    refused(&paul, escalation(STRONGER), "fournisseur propre").await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_escalation_stops_serving_once_the_conversations_provider_changes() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    own_provider(&paul, MAIN).await;
    assert_eq!(
        paul.put(ESCALATION, escalation(STRONGER)).await.status,
        StatusCode::OK
    );
    // The same model for both: nothing to escalate to.
    own_provider(&paul, STRONGER).await;
    let shown = paul.get(ESCALATION).await.json();
    assert!(
        shown["problem"].as_str().unwrap().contains("doit différer"),
        "{shown}"
    );
    // Another provider: the choice is kept, but no longer serves.
    let local = json!({ "provider": "ollama", "model": "llama3.3", "baseUrl": ENDPOINT });
    assert_eq!(paul.put("/admin/llm", local).await.status, StatusCode::OK);
    let shown = paul.get(ESCALATION).await.json();
    assert_eq!(shown["settings"]["model"], STRONGER);
    assert!(
        shown["problem"].as_str().unwrap().contains("a changé"),
        "{shown}"
    );
    // Back on the platform's model.
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
    let shown = paul.get(ESCALATION).await.json();
    assert!(
        shown["problem"]
            .as_str()
            .unwrap()
            .contains("fournisseur propre"),
        "{shown}"
    );
    assert_eq!(paul.delete(ESCALATION).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_administrators_of_its_settings_see_or_set_it() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    own_provider(&paul, MAIN).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.escalade").await;
    let partial = activate(&api, &mut partial).await;
    for client in [&lucas, &partial] {
        assert_eq!(client.get(ESCALATION).await.status, StatusCode::FORBIDDEN);
        let put = client.put(ESCALATION, escalation(STRONGER)).await;
        assert_eq!(put.status, StatusCode::FORBIDDEN);
        assert_eq!(
            client.delete(ESCALATION).await.status,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(paul.get(ESCALATION).await.json()["settings"], Value::Null);
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}

/// Records a model call of `role` that cost `micros`, `at` that time, for
/// the company `tenant_id`.
async fn spent(
    api: &TestApi,
    tenant_id: uuid::Uuid,
    role: &str,
    micros: i64,
    at: chrono::DateTime<Utc>,
) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query(
        "insert into usage_records (tenant_id, kind, role, provider, model, input_tokens, \
         output_tokens, cost_micros, created_at) \
         values ($1, 'llm', $2, 'anthropic', 'claude-opus-5-5', 10, 10, $3, $4)",
    )
    .bind(tenant_id)
    .bind(role)
    .bind(micros)
    .bind(at)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_month_s_spending_counts_the_mechanisms_alone_and_stays_with_the_company() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    own_provider(&paul, MAIN).await;
    assert_eq!(
        paul.put(ESCALATION, escalation(STRONGER)).await.status,
        StatusCode::OK
    );
    let before = paul.get(ESCALATION).await.json()["spentMicros"]
        .as_i64()
        .unwrap();
    let now = Utc::now();
    let last_month = month_start(now) - Duration::hours(1);
    for (role, micros, at) in [
        ("escalation", 2_000_000, now),
        ("classification", 500_000, now),
        ("advisor", 250_000, now),
        ("main", 10_000_000, now),
        ("review", 1_000_000, now),
        ("escalation", 5_000_000, last_month),
    ] {
        spent(&api, tenant::ID, role, micros, at).await;
    }
    // Another company's escalation is none of this one's.
    spent(&api, OTHER_TENANT_ID, "escalation", 7_000_000, now).await;
    let shown = paul.get(ESCALATION).await.json();
    assert_eq!(
        shown["spentMicros"].as_i64().unwrap() - before,
        2_750_000,
        "{shown}"
    );
    let usage = paul.get("/admin/usage").await.json();
    let escalation_spent = &usage["escalation"];
    assert_eq!(escalation_spent["budgetMicros"], 50_000_000, "{usage}");
    assert_eq!(escalation_spent["spentMicros"], shown["spentMicros"]);
    let roles: Vec<&str> = usage["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["role"].as_str())
        .collect();
    for role in ["escalation", "classification", "advisor", "main", "review"] {
        assert!(roles.contains(&role), "{role}: {usage}");
    }

    // Another company neither reads nor writes this one's escalation.
    let mut other = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let seen: i64 = sqlx::query_scalar("select count(*) from tenant_model_escalation")
        .fetch_one(&mut *other)
        .await
        .unwrap();
    assert_eq!(seen, 0);
    let written = sqlx::query(
        "insert into tenant_model_escalation (tenant_id, provider, model, \
         monthly_budget_micros, max_per_run) values ($1, 'anthropic', 'claude-opus-5-5', \
         1000000, 1)",
    )
    .bind(tenant::ID)
    .execute(&mut *other)
    .await;
    assert!(written.is_err(), "a row was written for another company");
    drop(other);

    assert_eq!(paul.delete(ESCALATION).await.status, StatusCode::OK);
    assert!(paul.get("/admin/usage").await.json()["escalation"].is_null());
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}
