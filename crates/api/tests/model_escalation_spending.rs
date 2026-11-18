//! What a company's escalation mechanisms spend this month (ADR-207): read
//! from the usage by role, what the company pays for each call (its cost on
//! its own key, the amount re-invoiced on the operator's, ADR-209), shown
//! with the usage, and kept to the company: another neither reads nor
//! writes it. The table keeps to the bounds the rules check.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_agent::llm::{
    MAX_ESCALATION_BUDGET_MICROS, MAX_ESCALATIONS_PER_RUN, MIN_ESCALATION_BUDGET_MICROS,
};
use mimorya_core::demo::{tenant, user};
use mimorya_core::time_zone::month_start;
use mimorya_db::testing::OTHER_TENANT_ID;
use support::TestApi;
use support::model_escalation::{ESCALATION, MAIN, STRONGER, escalation, own_provider, start};

/// Records a model call of `role` that cost `micros`, `at` that time, for
/// the company `tenant_id`, on its own key.
async fn spent(
    api: &TestApi,
    tenant_id: uuid::Uuid,
    role: &str,
    micros: i64,
    at: chrono::DateTime<Utc>,
) {
    recorded(api, tenant_id, role, micros, None, at).await;
}

/// The same, re-invoiced `billed` on the operator's key if some.
async fn recorded(
    api: &TestApi,
    tenant_id: uuid::Uuid,
    role: &str,
    micros: i64,
    billed: Option<i64>,
    at: chrono::DateTime<Utc>,
) {
    let formula = if billed.is_some() {
        "capped"
    } else {
        "client_key"
    };
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query(
        "insert into usage_records (tenant_id, kind, role, provider, model, input_tokens, \
         output_tokens, cost_micros, created_at, supply_formula, billed_micros) \
         values ($1, 'llm', $2, 'anthropic', 'claude-opus-5-5', 10, 10, $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind(role)
    .bind(micros)
    .bind(at)
    .bind(formula)
    .bind(billed)
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
    // On the operator's key, what the company pays: the amount re-invoiced.
    recorded(
        &api,
        tenant::ID,
        "escalation",
        1_000_000,
        Some(1_100_000),
        now,
    )
    .await;
    // Another company's escalation is none of this one's.
    spent(&api, OTHER_TENANT_ID, "escalation", 7_000_000, now).await;
    let shown = paul.get(ESCALATION).await.json();
    assert_eq!(
        shown["spentMicros"].as_i64().unwrap() - before,
        3_850_000,
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_table_keeps_to_the_bounds_the_rules_check() {
    let api = start().await;
    let insert = |budget: i64, per_run: u32| {
        let db = api.app.services.db.clone();
        async move {
            let mut tx = db.begin(tenant::ID).await.unwrap();
            let written = sqlx::query(
                "insert into tenant_model_escalation (tenant_id, provider, model, \
                 monthly_budget_micros, max_per_run) values ($1, 'anthropic', $2, $3, $4)",
            )
            .bind(tenant::ID)
            .bind(STRONGER)
            .bind(budget)
            .bind(i32::try_from(per_run).unwrap())
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string());
            // Rolled back: nothing is left for the other tests.
            drop(tx);
            written
        }
    };
    for (budget, per_run, constraint) in [
        (
            MIN_ESCALATION_BUDGET_MICROS - 1,
            1,
            "tenant_model_escalation_budget",
        ),
        (
            MAX_ESCALATION_BUDGET_MICROS + 1,
            1,
            "tenant_model_escalation_budget",
        ),
        (
            MIN_ESCALATION_BUDGET_MICROS,
            0,
            "tenant_model_escalation_per_run",
        ),
        (
            MIN_ESCALATION_BUDGET_MICROS,
            MAX_ESCALATIONS_PER_RUN + 1,
            "tenant_model_escalation_per_run",
        ),
    ] {
        let refused = insert(budget, per_run).await.unwrap_err();
        assert!(
            refused.contains(constraint),
            "{budget}, {per_run}: {refused}"
        );
    }
    for (budget, per_run) in [
        (MIN_ESCALATION_BUDGET_MICROS, 1),
        (MAX_ESCALATION_BUDGET_MICROS, MAX_ESCALATIONS_PER_RUN),
    ] {
        insert(budget, per_run).await.unwrap();
    }
}
