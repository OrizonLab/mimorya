//! The escalation follows the key of its company's runs, never another
//! (ADR-209). Chosen beside the platform's model in formula 2, it is built
//! with the operator's key alone, its calls counted in the cap. Once the
//! company gives its own provider of the same kind, the same choice is
//! built with the company's key alone, its calls recorded on that key,
//! outside the cap. That provider removed, it goes back to the operator's
//! key alone, counted in the cap again.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedProvider;
use mimorya_api::services::provisioning::{CapChoice, model_supply_report};
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::SupplyFormula;
use serde_json::json;
use sqlx::PgPool;
use support::llm::{
    API_KEY, ENDPOINT, ESCALATION_MODEL, MASTER_KEY, OPERATOR_KEY, SCRIPTED_CALL_MICROS,
};
use support::model_escalation::{ESCALATION, after_failure, charged, escalated_from, loaded};
use support::model_supply::{capped, supply};
use support::runs::start_run;
use support::{Client, TestApi, owner_url};

/// The platform's provider configured with the operator's key, and a
/// company able to keep its own key; each call priced.
async fn start() -> TestApi {
    TestApi::start_priced_with(&[
        ("LLM_PROVIDER", "anthropic"),
        ("ANTHROPIC_API_KEY", OPERATOR_KEY),
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
        ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
    ])
    .await
}

/// Waits until this process loaded the company's runs on its own key or
/// not, its escalation with them.
async fn on_its_own_key(api: &TestApi, own: bool) {
    for _ in 0..100 {
        let models = api.app.services.llm.resolve(tenant::ID).await.unwrap();
        if models.runs_on_operator_key() != own && models.escalation.is_some() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the company's models were never loaded again");
}

/// One run escalated after `main` failed its review: the keys the
/// escalation model was built with for it (since `from` configurations
/// built), its escalation call's formula and amount re-invoiced, and what
/// the cap counted meanwhile.
async fn escalated_run(
    api: &TestApi,
    owner: &PgPool,
    marc: &Client<'_>,
    main: &ScriptedProvider,
    from: usize,
) -> (Vec<String>, (String, Option<i64>), i64) {
    let before = model_supply_report(owner, tenant::SLUG).await.unwrap();
    escalated_from(main, api);
    let run = start_run(marc).await;
    api.idle().await;
    let after = model_supply_report(owner, tenant::SLUG).await.unwrap();
    let keys = api.built.lock().unwrap()[from..]
        .iter()
        .filter(|config| config.model.as_deref() == Some(ESCALATION_MODEL))
        .map(|config| config.api_key.clone().unwrap_or_default())
        .collect();
    let calls = charged(api, run).await;
    let (role, formula, _, billed) = calls.last().unwrap().clone();
    assert_eq!(role, "escalation", "{calls:?}");
    let counted = after.spending.capped_billed_micros - before.spending.capped_billed_micros;
    (keys, (formula, billed), counted)
}

fn built(api: &TestApi) -> usize {
    api.built.lock().unwrap().len()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_escalation_is_built_with_the_key_of_the_runs_never_another() {
    let api = start().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    supply(&owner, capped(CapChoice::Default)).await;
    let platform = api.model.as_ref().unwrap().clone();
    let on_cap = ("capped".to_string(), Some(SCRIPTED_CALL_MICROS));

    // Beside the platform's model: the operator's key alone, in the cap.
    let from = built(&api);
    let set = paul.put(ESCALATION, after_failure()).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    loaded(&api, SupplyFormula::Capped, true).await;
    let (keys, call, counted) = escalated_run(&api, &owner, &marc, &platform, from).await;
    assert!(!keys.is_empty() && keys.iter().all(|key| key == OPERATOR_KEY));
    assert_eq!((call, counted), (on_cap.clone(), 4 * SCRIPTED_CALL_MICROS));

    // Its own provider of the same kind: the same choice, kept, built with
    // the company's key alone, outside the cap.
    let from = built(&api);
    let own = json!({ "provider": "anthropic", "model": "claude-haiku-5-5", "apiKey": API_KEY });
    let set = paul.put("/admin/llm", own).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    on_its_own_key(&api, true).await;
    let company = api.company_model.clone();
    let (keys, call, counted) = escalated_run(&api, &owner, &marc, &company, from).await;
    assert!(!keys.is_empty() && keys.iter().all(|key| key == API_KEY));
    assert_eq!((call, counted), (("client_key".to_string(), None), 0));

    // That provider removed: back to the operator's key alone.
    let from = built(&api);
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
    on_its_own_key(&api, false).await;
    let (keys, call, counted) = escalated_run(&api, &owner, &marc, &platform, from).await;
    assert!(!keys.is_empty() && keys.iter().all(|key| key == OPERATOR_KEY));
    assert_eq!((call, counted), (on_cap, 4 * SCRIPTED_CALL_MICROS));
    assert_eq!(paul.delete(ESCALATION).await.status, StatusCode::OK);
}
