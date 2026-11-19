//! The operator's supply as the company's administrators read it (ADR-210):
//! its formula, its cap and what the month was re-invoiced under it, or its
//! credit's balance and payments, the latest first, and whether the supply
//! holds its runs back now, where they go: the operator's key, its own
//! provider (nothing then held back) or no model at all. What the company
//! pays, never the operator's real cost nor its markup. Only those who
//! manage its settings company-wide read it, another company's credit never
//! among its own. Its usage reads, on the operator's key, the amounts
//! re-invoiced.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::provisioning::{
    CapChoice, SupplyChange, change_model_supply, model_supply_report,
};
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::{SupplyFormula, SupplyStop};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::accounts::{activate, create_partial_admin};
use support::llm::{API_KEY, MASTER_KEY};
use support::model_supply::{CAP, capped, spent, supply};
use support::{TestApi, owner_url};

const SUPPLY: &str = "/admin/model-supply";

/// A call on the operator's key under `formula`, that cost `cost_micros`
/// and was re-invoiced `billed_micros`.
async fn recorded(owner: &PgPool, formula: SupplyFormula, cost_micros: i64, billed_micros: i64) {
    sqlx::query(
        "insert into usage_records (tenant_id, kind, role, provider, model, cost_micros, \
         supply_formula, billed_micros) values ($1, 'llm', 'main', 'anthropic', \
         'claude-haiku-5-5', $2, $3, $4)",
    )
    .bind(tenant::ID)
    .bind(cost_micros)
    .bind(formula.as_str())
    .bind(billed_micros)
    .execute(owner)
    .await
    .unwrap();
}

fn paid(amount_micros: i64, reference: &str) -> SupplyChange {
    SupplyChange::Credit {
        amount_micros,
        reference: reference.into(),
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_reads_its_cap_or_its_credit_as_it_pays_them() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let paul = api.sign_in(user::PAUL).await;
    supply(&owner, capped(CapChoice::Amount(CAP))).await;
    supply(&owner, SupplyChange::Markup { bps: 1_000 }).await;
    // Re-invoiced 600,000 for a real cost of 400,000: the former is read.
    spent(&owner, SupplyFormula::Capped, 0).await;
    recorded(&owner, SupplyFormula::Capped, 400_000, 600_000).await;

    let read = paul.get(SUPPLY).await;
    assert_eq!(read.status, StatusCode::OK);
    let shown = read.json();
    assert_eq!(shown["formula"], "capped", "{shown}");
    assert_eq!(shown["runs"], "operatorKey");
    assert_eq!(shown["monthlyCapMicros"], CAP);
    assert_eq!(shown["unlimited"], false);
    assert_eq!(shown["monthBilledMicros"], 600_000);
    assert_eq!(shown["balanceMicros"], Value::Null);
    assert_eq!(shown["stopped"], Value::Null);
    // What the company pays: neither the operator's real cost nor its markup.
    let text = read.text();
    assert!(!text.contains("markup") && !text.contains("cost"), "{text}");
    // Past the cap, it says why.
    spent(&owner, SupplyFormula::Capped, CAP).await;
    let shown = paul.get(SUPPLY).await.json();
    assert_eq!(shown["stopped"], SupplyStop::MonthlyCap.message());
    // On its own provider, nothing of the operator's is held back.
    let own = json!({ "provider": "anthropic", "model": "claude-haiku-5-5", "apiKey": API_KEY });
    assert_eq!(paul.put("/admin/llm", own).await.status, StatusCode::OK);
    let shown = paul.get(SUPPLY).await.json();
    assert_eq!(
        (&shown["runs"], &shown["stopped"]),
        (&json!("ownProvider"), &Value::Null)
    );
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);

    // A prepaid credit: its balance and payments, the latest first; another
    // company's never among them.
    let prepaid = SupplyChange::Formula {
        formula: SupplyFormula::Prepaid,
        cap: CapChoice::Default,
    };
    supply(&owner, prepaid).await;
    supply(&owner, paid(2_000_000, "FAC-2026-0201")).await;
    supply(&owner, paid(1_000_000, "FAC-2026-0302")).await;
    change_model_supply(&owner, "autre-entreprise", paid(9_000_000, "FAC-AUTRE"))
        .await
        .unwrap();
    // Re-invoiced 500,000 for a real cost of 300,000: the balance counts the former.
    spent(&owner, SupplyFormula::Prepaid, 0).await;
    recorded(&owner, SupplyFormula::Prepaid, 300_000, 500_000).await;
    let report = model_supply_report(&owner, tenant::SLUG).await.unwrap();
    let shown = paul.get(SUPPLY).await.json();
    assert_eq!(shown["formula"], "prepaid", "{shown}");
    assert_eq!(shown["balanceMicros"], report.spending.balance_micros());
    assert_eq!(shown["balanceMicros"], 2_500_000);
    let payments: Vec<(i64, String)> = shown["payments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let amount = p["amountMicros"].as_i64().unwrap();
            (amount, p["reference"].as_str().unwrap().to_string())
        })
        .collect();
    assert_eq!(
        payments,
        [
            (1_000_000, "FAC-2026-0302".to_string()),
            (2_000_000, "FAC-2026-0201".to_string())
        ]
    );
    assert_eq!(shown["stopped"], Value::Null);

    spent(&owner, SupplyFormula::Capped, 0).await;
    supply(&owner, SupplyChange::Markup { bps: 0 }).await;
    supply(&owner, capped(CapChoice::Default)).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn on_a_server_without_the_platform_s_model_its_runs_go_nowhere() {
    let api = TestApi::without_llm().await;
    let paul = api.sign_in(user::PAUL).await;
    let shown = paul.get(SUPPLY).await.json();
    assert_eq!(shown["formula"], "capped", "{shown}");
    assert_eq!(
        (&shown["runs"], &shown["stopped"]),
        (&json!("none"), &Value::Null)
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_administrators_of_its_settings_read_it() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.fourniture").await;
    let partial = activate(&api, &mut partial).await;
    for client in [&lucas, &partial] {
        assert_eq!(client.get(SUPPLY).await.status, StatusCode::FORBIDDEN);
    }
    assert_eq!(paul.get(SUPPLY).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_usage_on_the_operator_s_key_reads_the_amounts_re_invoiced() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    for (formula, billed) in [("client_key", None), ("capped", Some(1_100_000_i64))] {
        sqlx::query(
            "insert into usage_records (tenant_id, kind, role, provider, model, input_tokens, \
             output_tokens, cost_micros, supply_formula, billed_micros) \
             values ($1, 'llm', 'main', 'anthropic', 'modele-releve', 10, 10, 1000000, $2, $3)",
        )
        .bind(tenant::ID)
        .bind(formula)
        .bind(billed)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    let usage = paul.get("/admin/usage?days=30").await.json();
    let mut rows: Vec<(bool, i64)> = usage["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["model"] == "modele-releve")
        .map(|row| {
            let operator = row["onOperatorKey"].as_bool().unwrap();
            (operator, row["costMicros"].as_i64().unwrap())
        })
        .collect();
    rows.sort();
    // Its own key's estimated cost; on the operator's, what it is re-invoiced.
    assert_eq!(rows, [(false, 1_000_000), (true, 1_100_000)], "{usage}");
    let text = json!(usage).to_string();
    assert!(!text.contains("billed"), "{text}");
}
