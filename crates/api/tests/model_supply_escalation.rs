//! A company's model escalation on the operator's key (ADR-209): in the
//! formulas 2 and 3, a company on the platform's model chooses one beside
//! it, built with the platform's provider and the operator's key, never
//! another. Each escalated call is recorded once, on the operator's key,
//! and counts both in the escalation's budget, at the amount re-invoiced,
//! and in the cap or the credit. Past the cap, the escalation model is not
//! called either. Without a formula of the operator nor a provider of its
//! own, a company may choose none, and one chosen stops serving once the
//! operator withdraws the platform's model.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::provisioning::{CapChoice, SupplyChange, model_supply_report};
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::{SupplyFormula, SupplyStop};
use serde_json::Value;
use sqlx::PgPool;
use support::llm::{ESCALATION_MODEL, OPERATOR_KEY, SCRIPTED_CALL_MICROS};
use support::model_escalation::{ESCALATION, after_failure, charged, escalated, loaded, refused};
use support::model_supply::{CAP, capped, spent, supply};
use support::runs::start_run;
use support::{TestApi, owner_url};

/// 10 %: what a call is re-invoiced, beside its cost.
const MARKUP_BPS: u32 = 1_000;
const BILLED: i64 = SCRIPTED_CALL_MICROS + SCRIPTED_CALL_MICROS / 10;

/// The platform's model configured with the operator's key, as `LLM_*`
/// set it in production; each call of the scripted model priced.
async fn start() -> TestApi {
    TestApi::start_priced_with(&[
        ("LLM_PROVIDER", "anthropic"),
        ("ANTHROPIC_API_KEY", OPERATOR_KEY),
    ])
    .await
}

fn formula(formula: SupplyFormula) -> SupplyChange {
    SupplyChange::Formula {
        formula,
        cap: CapChoice::Default,
    }
}

fn spent_micros(view: &Value) -> i64 {
    view["spentMicros"].as_i64().unwrap()
}

async fn leave(api: &TestApi, owner: &PgPool) {
    let paul = api.sign_in(user::PAUL).await;
    paul.delete(ESCALATION).await;
    supply(owner, SupplyChange::Markup { bps: 0 }).await;
    supply(owner, capped(CapChoice::Default)).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_escalation_beside_the_platform_s_model_counts_in_its_budget_and_in_the_supply() {
    let api = start().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    supply(&owner, SupplyChange::Markup { bps: MARKUP_BPS }).await;
    let paid = SupplyChange::Credit {
        amount_micros: 100_000_000,
        reference: "FAC-2026-1119".into(),
    };
    supply(&owner, paid).await;

    for chosen in [SupplyFormula::Capped, SupplyFormula::Prepaid] {
        supply(&owner, formula(chosen)).await;
        let shown = paul.get(ESCALATION).await.json();
        assert_eq!(shown["mainModel"], "scripted-model", "{shown}");
        assert_eq!(shown["onOperatorKey"], true, "{shown}");
        let set = paul.put(ESCALATION, after_failure()).await;
        assert_eq!(set.status, StatusCode::OK, "{}", set.text());
        loaded(&api, chosen, true).await;
        let budget_before = spent_micros(&paul.get(ESCALATION).await.json());
        let before = model_supply_report(&owner, tenant::SLUG).await.unwrap();

        escalated(&api);
        let run = start_run(&marc).await;
        api.idle().await;
        // Each call recorded once, on the operator's key.
        let call = |role: &str| {
            let formula = chosen.as_str().to_string();
            (
                role.to_string(),
                formula,
                SCRIPTED_CALL_MICROS,
                Some(BILLED),
            )
        };
        assert_eq!(
            charged(&api, run).await,
            [call("main"), call("main"), call("main"), call("escalation")],
            "{chosen:?}"
        );
        // In the budget, at what the company pays for it…
        let budget_after = spent_micros(&paul.get(ESCALATION).await.json());
        assert_eq!(budget_after - budget_before, BILLED, "{chosen:?}");
        // …and in the cap or the credit, with the run's other calls.
        let after = model_supply_report(&owner, tenant::SLUG).await.unwrap();
        let counted = match chosen {
            SupplyFormula::Capped => {
                after.spending.capped_billed_micros - before.spending.capped_billed_micros
            }
            _ => before.spending.balance_micros() - after.spending.balance_micros(),
        };
        assert_eq!(counted, 4 * BILLED, "{chosen:?}");
    }
    // Built with the platform's provider and the operator's key alone.
    let built = api.built.lock().unwrap().clone();
    let escalations: Vec<_> = built
        .iter()
        .filter(|config| config.model.as_deref() == Some(ESCALATION_MODEL))
        .collect();
    assert!(!escalations.is_empty());
    for config in escalations {
        assert_eq!(config.kind, "anthropic");
        assert_eq!(config.api_key.as_deref(), Some(OPERATOR_KEY));
    }
    leave(&api, &owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn past_the_cap_the_escalation_model_is_not_called_either() {
    let api = start().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    supply(&owner, capped(CapChoice::Amount(CAP))).await;
    let set = paul.put(ESCALATION, after_failure()).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    loaded(&api, SupplyFormula::Capped, true).await;
    // Room for the main model's three calls, the third going past the cap.
    let room = 3 * SCRIPTED_CALL_MICROS - SCRIPTED_CALL_MICROS / 2;
    spent(&owner, SupplyFormula::Capped, CAP - room).await;

    escalated(&api);
    let run = start_run(&marc).await;
    api.idle().await;
    let read = marc.get(&format!("/runs/{run}")).await.json()["run"].clone();
    assert_eq!(read["error"], SupplyStop::MonthlyCap.message(), "{read}");
    assert!(api.escalation_model.requests().is_empty());
    assert_eq!(charged(&api, run).await.len(), 3);
    spent(&owner, SupplyFormula::Capped, 0).await;
    leave(&api, &owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_the_operator_s_formula_nor_its_own_provider_a_company_escalates_to_nothing() {
    let api = start().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let paul = api.sign_in(user::PAUL).await;
    supply(&owner, capped(CapChoice::Default)).await;
    let set = paul.put(ESCALATION, after_failure()).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    loaded(&api, SupplyFormula::Capped, true).await;

    // The operator withdraws the platform's model: the escalation chosen
    // beside it stops serving, saying why, and none may be chosen.
    supply(&owner, formula(SupplyFormula::ClientKey)).await;
    loaded(&api, SupplyFormula::ClientKey, false).await;
    let shown = paul.get(ESCALATION).await.json();
    assert_eq!(shown["mainModel"], Value::Null, "{shown}");
    assert_eq!(shown["onOperatorKey"], false, "{shown}");
    let problem = shown["problem"].as_str().unwrap();
    assert!(problem.contains("modèle de la plateforme"), "{problem}");
    refused(&paul, after_failure(), "fournisseur propre").await;

    // Supplied again, it serves again.
    supply(&owner, capped(CapChoice::Default)).await;
    loaded(&api, SupplyFormula::Capped, true).await;
    leave(&api, &owner).await;
}
