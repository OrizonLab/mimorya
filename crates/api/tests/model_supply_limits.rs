//! The limits of the operator's supply (ADR-208, step 1): past a company's
//! monthly cap or prepaid credit, no run is launched, its user told why,
//! nor a connection test made, and the refusal is traced; once the cap is
//! raised, runs go on. A run under way stops before its next call to the
//! model, saying why, past its cap as past its credit. Two runs at once
//! near the cap overrun it by their calls in flight at most. The company's
//! own key is never held back.

mod support;

use std::sync::{Arc, Barrier};

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_api::services::provisioning::{CapChoice, SupplyChange, model_supply_report};
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::{SupplyFormula, SupplyStop};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::llm::{API_KEY, MASTER_KEY, SCRIPTED_CALL_MICROS, ask};
use support::model_supply::{CAP, capped, credit_left, spent, supply};
use support::refusals::{entries_since, latest_entry};
use support::runs::start_run;
use support::{Client, TestApi, owner_url};
use tokio::task::block_in_place;
use uuid::Uuid;

/// Waits until this process loaded the company's own key for its runs.
async fn on_its_own_key(api: &TestApi) {
    for _ in 0..100 {
        let models = api.app.services.llm.resolve(tenant::ID).await.unwrap();
        if !models.runs_on_operator_key() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the company's own key was never loaded");
}

async fn run_of(client: &Client<'_>, run: Uuid) -> Value {
    client.get(&format!("/runs/{run}")).await.json()["run"].clone()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn past_its_cap_or_its_credit_no_run_is_launched_and_the_refusal_is_traced() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let since = latest_entry(&api).await;

    supply(&owner, capped(CapChoice::Amount(CAP))).await;
    spent(&owner, SupplyFormula::Capped, CAP).await;
    let (status, answer) = ask(&api, &marc, "Bonjour").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(answer.is_empty());
    let refused = marc.post("/conversations", json!({})).await.json()["conversation"]["id"]
        .as_str()
        .map(|id| format!("/conversations/{id}/messages"))
        .unwrap();
    let said = marc.post(&refused, json!({ "content": "Bonjour" })).await;
    assert_eq!(said.error_message(), SupplyStop::MonthlyCap.message());
    // Nor any test of the connection on the operator's key.
    let tested = paul.post("/admin/llm/test", json!({})).await;
    assert_eq!(tested.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(api.model.as_ref().unwrap().requests().is_empty());
    // Each refusal traced, the test's by whom asked for it.
    let audit = entries_since(&api, since).await;
    assert_eq!(audit.matches("model_supply.limited").count(), 3, "{audit}");
    assert!(audit.contains(&user::PAUL.to_string()), "{audit}");

    // A prepaid credit spent: the same.
    supply(
        &owner,
        SupplyChange::Formula {
            formula: SupplyFormula::Prepaid,
            cap: CapChoice::Default,
        },
    )
    .await;
    credit_left(&owner, 0).await;
    let said = marc.post(&refused, json!({ "content": "Bonjour" })).await;
    assert_eq!(said.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(said.error_message(), SupplyStop::CreditSpent.message());
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("model_supply.limited"), "{audit}");
    assert!(
        audit.contains("monthly_cap") && audit.contains("credit_spent"),
        "{audit}"
    );

    // Without a cap, at the operator's explicit asking, runs go on.
    supply(&owner, capped(CapChoice::Unlimited)).await;
    api.script(vec![ScriptedStep::text("Bonjour Marc.")]);
    let (status, answer) = ask(&api, &marc, "Bonjour").await;
    assert_eq!(
        (status, answer.as_str()),
        (StatusCode::ACCEPTED, "Bonjour Marc.")
    );
    supply(&owner, capped(CapChoice::Default)).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_under_way_stops_before_its_next_call_saying_why() {
    // Each call of the scripted model costs `SCRIPTED_CALL_MICROS`.
    let api = TestApi::start_priced_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let marc = api.sign_in(user::MARC).await;
    // Room left for one call, a little: it is made, and passes the cap.
    supply(&owner, capped(CapChoice::Amount(CAP))).await;
    spent(
        &owner,
        SupplyFormula::Capped,
        CAP - SCRIPTED_CALL_MICROS / 2,
    )
    .await;
    let since = latest_entry(&api).await;

    // An empty answer, refused by the review: the run would ask again.
    api.script(vec![ScriptedStep::text(""), ScriptedStep::text("jamais")]);
    let run = start_run(&marc).await;
    api.idle().await;
    let read = run_of(&marc, run).await;
    assert_eq!(read["status"], "failed", "{read}");
    assert_eq!(read["error"], SupplyStop::MonthlyCap.message());
    assert_eq!(api.model.as_ref().unwrap().requests().len(), 1);
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("model_supply.limited"), "{audit}");
    supply(&owner, capped(CapChoice::Default)).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_on_a_spent_credit_stops_before_its_next_call() {
    let api = TestApi::start_priced_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let marc = api.sign_in(user::MARC).await;
    let prepaid = SupplyChange::Formula {
        formula: SupplyFormula::Prepaid,
        cap: CapChoice::Default,
    };
    supply(&owner, prepaid).await;
    let paid = SupplyChange::Credit {
        amount_micros: CAP,
        reference: "FAC-2026-1118".into(),
    };
    supply(&owner, paid).await;
    // Half a call left: the first is made, and spends the credit.
    credit_left(&owner, SCRIPTED_CALL_MICROS / 2).await;
    let since = latest_entry(&api).await;

    api.script(vec![ScriptedStep::text(""), ScriptedStep::text("jamais")]);
    let run = start_run(&marc).await;
    api.idle().await;
    let read = run_of(&marc, run).await;
    assert_eq!(read["status"], "failed", "{read}");
    assert_eq!(read["error"], SupplyStop::CreditSpent.message());
    assert_eq!(api.model.as_ref().unwrap().requests().len(), 1);
    let report = model_supply_report(&owner, tenant::SLUG).await.unwrap();
    assert_eq!(report.spending.balance_micros(), -SCRIPTED_CALL_MICROS / 2);
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("credit_spent"), "{audit}");
    supply(&owner, capped(CapChoice::Default)).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires PostgreSQL"]
async fn two_runs_at_once_near_the_cap_overrun_it_by_their_calls_in_flight_at_most() {
    let api = TestApi::start_priced_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let marc = api.sign_in(user::MARC).await;
    supply(&owner, capped(CapChoice::Amount(CAP))).await;
    let before = CAP - SCRIPTED_CALL_MICROS / 2;
    spent(&owner, SupplyFormula::Capped, before).await;

    // Each run's first call is held until the other's is in flight too:
    // both were allowed, neither counted yet.
    let both = Arc::new(Barrier::new(2));
    let held = |both: Arc<Barrier>| {
        ScriptedStep::dynamic(move |_| {
            block_in_place(|| both.wait());
            ScriptedReply::text("")
        })
    };
    api.script(vec![held(both.clone()), held(both)]);
    let runs = [start_run(&marc).await, start_run(&marc).await];
    api.idle().await;

    let report = model_supply_report(&owner, tenant::SLUG).await.unwrap();
    let overrun = report.spending.capped_billed_micros - CAP;
    // Past the cap by what the two calls in flight cost, at most.
    assert!(
        overrun > 0 && overrun <= 2 * SCRIPTED_CALL_MICROS,
        "{overrun}"
    );
    for run in runs {
        let read = run_of(&marc, run).await;
        assert_eq!(read["error"], SupplyStop::MonthlyCap.message(), "{read}");
    }
    assert_eq!(api.model.as_ref().unwrap().requests().len(), 2);
    supply(&owner, capped(CapChoice::Default)).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_s_own_key_is_never_held_back() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    supply(&owner, capped(CapChoice::Amount(CAP))).await;
    spent(&owner, SupplyFormula::Capped, CAP).await;
    let own = json!({ "provider": "anthropic", "model": "claude-haiku-5-5", "apiKey": API_KEY });
    assert_eq!(paul.put("/admin/llm", own).await.status, StatusCode::OK);
    on_its_own_key(&api).await;

    api.company_model
        .push([ScriptedStep::text("Sur la clé de l’entreprise.")]);
    let (status, answer) = ask(&api, &marc, "Bonjour").await;
    assert_eq!(
        (status, answer.as_str()),
        (StatusCode::ACCEPTED, "Sur la clé de l’entreprise.")
    );
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
    supply(&owner, capped(CapChoice::Default)).await;
}
