//! The router's classification asked of the company's decision service
//! (ADR-213, D1), its main model the engine: active, its choice starts the
//! run on the escalation model, recorded with the role « décision » in the
//! run and counted in its cost, journaled as a decision; observed, the
//! decision is asked and counted, and the main model's classification
//! routes the run as before; off, nothing is asked. (The rules of the
//! decision itself: `crates/agent/tests/runtime_decision_routing.rs`.)

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::decisions::{DECISIONS, set, settings};
use support::llm::{API_KEY, ENDPOINT, ESCALATION_MODEL, MASTER_KEY, SCRIPTED_CALL_MICROS};
use support::refusals::{entries_since, latest_entry};
use support::runs::start_run;
use support::usage_roles::recorded_roles;
use support::{Client, TestApi};
use uuid::Uuid;

const ESCALATION: &str = "/admin/model-escalation";
const ANSWER: &str = "Trois interventions cette semaine.";
const ENV: [(&str, &str); 2] = [
    ("MIMORYA_MASTER_KEY", MASTER_KEY),
    ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
];

/// The company on its own provider, its conversations routed by
/// classification.
async fn routing(paul: &Client<'_>) {
    let own = json!({ "provider": "anthropic", "model": "claude-haiku-5-5", "apiKey": API_KEY });
    assert_eq!(paul.put("/admin/llm", own).await.status, StatusCode::OK);
    let chosen = json!({
        "model": ESCALATION_MODEL,
        "afterFailure": false,
        "router": true,
        "advisor": false,
        "routes": { "conversation": "classification", "mission": "main",
                    "scheduledTask": "main", "hook": "main" },
        "monthlyBudgetMicros": 50_000_000,
        "maxPerRun": 5,
    });
    let set_ = paul.put(ESCALATION, chosen).await;
    assert_eq!(set_.status, StatusCode::OK, "{}", set_.text());
}

/// The model's routing set to `mode` at `threshold`, by the main model.
fn routed_by(mode: &str, threshold: f64) -> Value {
    let mut body = settings("mainModel", None);
    body["usages"][0] = json!({ "key": "modelRouting", "mode": mode, "threshold": threshold });
    body
}

/// Why `run` escalated, as journaled.
async fn started_because(api: &TestApi, run: Uuid) -> Vec<String> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select reason from audit_log where run_id = $1 \
         and action = 'model_escalation.started' order by id",
    )
    .bind(run)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

fn decided(level: &str, certainty: &str) -> ScriptedStep {
    ScriptedStep::text(
        json!({ "niveau": { "reponse": level, "certitude": certainty } }).to_string(),
    )
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_decision_routes_a_conversation_and_is_counted_in_its_run() {
    let api = TestApi::start_priced_with(&ENV).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    routing(&paul).await;
    set(&paul, routed_by("active", 0.7)).await;
    let since = latest_entry(&api).await;

    // « principal » said with a mean certainty: below the threshold, escalated.
    api.company_model.push([decided("principal", "moyenne")]);
    api.escalation_model.push([ScriptedStep::text(ANSWER)]);
    let run = start_run(&marc).await;
    api.idle().await;
    assert_eq!(recorded_roles(&api, run).await, ["decision", "escalation"]);
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("model_escalation.started"), "{audit}");
    assert_eq!(started_because(&api, run).await, ["decision"]);
    // The decision's cost in the run's own, as its other calls.
    let detail = marc.get(&format!("/runs/{run}")).await.json();
    assert_eq!(
        detail["run"]["usage"]["costMicros"],
        2 * SCRIPTED_CALL_MICROS,
        "{detail}"
    );

    // « principal » said with certainty: the main model answers, unclassified.
    api.company_model
        .push([decided("principal", "forte"), ScriptedStep::text(ANSWER)]);
    let run = start_run(&marc).await;
    api.idle().await;
    assert_eq!(recorded_roles(&api, run).await, ["decision", "main"]);

    // Observed: asked and counted, the classification routes the run.
    set(&paul, routed_by("observe", 0.7)).await;
    api.company_model.push([
        decided("escalade", "forte"),
        ScriptedStep::text(r#"{"niveau": "principal"}"#),
        ScriptedStep::text(ANSWER),
    ]);
    let run = start_run(&marc).await;
    api.idle().await;
    assert_eq!(
        recorded_roles(&api, run).await,
        ["decision", "classification", "main"]
    );

    // Off: nothing asked of the decision service.
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
    api.company_model.push([
        ScriptedStep::text(r#"{"niveau": "principal"}"#),
        ScriptedStep::text(ANSWER),
    ]);
    let run = start_run(&marc).await;
    api.idle().await;
    assert_eq!(recorded_roles(&api, run).await, ["classification", "main"]);
    assert_eq!(api.escalation_model.requests().len(), 1);
    assert_eq!(paul.delete(ESCALATION).await.status, StatusCode::OK);
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}
