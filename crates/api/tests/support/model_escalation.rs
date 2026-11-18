//! A company's model escalation as its tests choose it (ADR-207): its own
//! provider, an escalation of its model, a choice refused; its models
//! loaded again, a run escalated after a failed review, and what its calls
//! were recorded.

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedProvider, ScriptedStep};
use mimorya_core::demo::tenant;
use mimorya_core::model_supply::SupplyFormula;
use serde_json::{Value, json};
use uuid::Uuid;

use super::llm::{API_KEY, ENDPOINT, ESCALATION_MODEL, MASTER_KEY};
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

/// Waits until this process loaded the company's formula and whether it
/// has an escalation.
pub async fn loaded(api: &TestApi, formula: SupplyFormula, escalating: bool) {
    for _ in 0..100 {
        let models = api.app.services.llm.resolve(tenant::ID).await.unwrap();
        if models.supply.formula == formula && models.escalation.is_some() == escalating {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the company's models were never loaded again");
}

/// After a failed review, to `ESCALATION_MODEL`.
pub fn after_failure() -> Value {
    let mut chosen = escalation(ESCALATION_MODEL);
    chosen["afterFailure"] = json!(true);
    chosen
}

/// The platform's model answers nothing, then twice again when corrected;
/// the escalation model answers.
pub fn escalated(api: &TestApi) {
    escalated_from(api.model.as_ref().expect("scripted model"), api);
}

/// The same, `main` being the model of the company's runs.
pub fn escalated_from(main: &ScriptedProvider, api: &TestApi) {
    main.push((0..3).map(|_| ScriptedStep::text("")));
    api.escalation_model
        .push([ScriptedStep::text("Trois interventions cette semaine.")]);
}

/// The model calls of `run`: role, formula, cost and amount re-invoiced.
pub async fn charged(api: &TestApi, run: Uuid) -> Vec<(String, String, i64, Option<i64>)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select role, supply_formula, cost_micros, billed_micros from usage_records \
         where run_id = $1 and kind = 'llm' order by id",
    )
    .bind(run)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}
