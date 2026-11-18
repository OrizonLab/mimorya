//! A company's runs escalate after a failed review (ADR-207): once its main
//! model's corrections fail, the run goes on with its escalation model,
//! recorded as such, its cost counted in the month's spending; past the month's budget, it does not, and says so
//! in the audit log; removed, the escalation no longer serves its runs.

mod support;

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::llm::{API_KEY, ENDPOINT, ESCALATION_MODEL, MASTER_KEY};
use support::refusals::{entries_since, latest_entry};
use support::runs::start_run;
use support::usage_roles::recorded_roles;
use support::{Client, TestApi};

const ESCALATION: &str = "/admin/model-escalation";
const EMPTY: &str = "La réponse est vide.";

async fn start() -> TestApi {
    TestApi::start_with(&[
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
        ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
    ])
    .await
}

/// The company on its own provider, escalating after a failed review within
/// `budget_micros` a month.
async fn escalating(paul: &Client<'_>, budget_micros: i64) {
    let own = json!({ "provider": "anthropic", "model": "claude-haiku-5-5", "apiKey": API_KEY });
    assert_eq!(paul.put("/admin/llm", own).await.status, StatusCode::OK);
    let chosen = paul
        .put(
            ESCALATION,
            json!({
                "model": ESCALATION_MODEL,
                "afterFailure": true,
                "router": false,
                "advisor": false,
                "monthlyBudgetMicros": budget_micros,
                "maxPerRun": 5,
            }),
        )
        .await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text());
}

async fn leave(paul: &Client<'_>) {
    paul.delete(ESCALATION).await;
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}

/// The main model answers nothing, then twice again when corrected.
fn empty_answers(api: &TestApi) {
    api.company_model
        .push((0..3).map(|_| ScriptedStep::text("")));
}

async fn verification(marc: &Client<'_>, run: uuid::Uuid) -> Value {
    marc.get(&format!("/runs/{run}")).await.json()["run"]["verification"].clone()
}

fn spent(view: &Value) -> i64 {
    view["spentMicros"].as_i64().unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_whose_answer_fails_its_review_goes_on_with_the_escalation_model() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    escalating(&paul, 50_000_000).await;
    let before = spent(&paul.get(ESCALATION).await.json());
    let since = latest_entry(&api).await;

    empty_answers(&api);
    api.escalation_model
        .push([ScriptedStep::text("Trois interventions cette semaine.")]);
    let run = start_run(&marc).await;
    api.idle().await;
    assert_eq!(
        verification(&marc, run).await,
        json!({ "passed": true, "issues": [], "judged": false })
    );
    assert_eq!(api.escalation_model.requests().len(), 1);
    assert_eq!(
        recorded_roles(&api, run).await,
        ["main", "main", "main", "escalation"]
    );
    // Counted in the month's spending, at what the escalation call cost.
    let escalated: i64 = {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        sqlx::query_scalar(
            "select cost_micros from usage_records where run_id = $1 and role = 'escalation'",
        )
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
    };
    assert_eq!(
        spent(&paul.get(ESCALATION).await.json()) - before,
        escalated
    );
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("model_escalation.started"), "{audit}");

    // Removed: the company's runs no longer escalate.
    assert_eq!(paul.delete(ESCALATION).await.status, StatusCode::OK);
    empty_answers(&api);
    let run = start_run(&marc).await;
    api.idle().await;
    assert_eq!(verification(&marc, run).await["issues"], json!([EMPTY]));
    assert_eq!(api.escalation_model.requests().len(), 1);
    leave(&paul).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn past_the_month_s_budget_a_run_keeps_its_main_model_and_it_is_audited() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    escalating(&paul, 1_000_000).await;
    // The month's escalations already cost more than its budget.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "insert into usage_records (tenant_id, kind, role, provider, model, input_tokens, \
         output_tokens, cost_micros, created_at) \
         values ($1, 'llm', 'escalation', 'anthropic', $2, 10, 10, 2000000, $3)",
    )
    .bind(tenant::ID)
    .bind(ESCALATION_MODEL)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let since = latest_entry(&api).await;

    empty_answers(&api);
    let run = start_run(&marc).await;
    api.idle().await;
    assert_eq!(verification(&marc, run).await["issues"], json!([EMPTY]));
    assert!(api.escalation_model.requests().is_empty());
    assert!(
        !recorded_roles(&api, run)
            .await
            .contains(&"escalation".to_string())
    );
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("model_escalation.limited"), "{audit}");
    assert!(audit.contains("monthly_budget"), "{audit}");
    leave(&paul).await;
}
