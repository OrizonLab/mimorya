//! How the operator supplies a company's model (ADR-208), through the
//! commands of `mimorya-admin` as the database's owner: each change is
//! traced in the company's audit log by the `system` actor, refused out of
//! its bounds, a payment never recorded twice and a correction a payment of
//! its own; the balance is computed again from the register. A company the
//! operator does not supply has no platform model, and its administrator
//! cannot give it one: choosing its own key and removing it leaves it
//! without. Each call carries the formula it was made under and, on the
//! operator's key, its re-invoiced amount, the markup never changing its
//! real cost.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::provisioning::{
    CapChoice, SupplyChange, change_model_supply, model_supply_report,
};
use mimorya_core::DomainError;
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::SupplyFormula;
use serde_json::{Value, json};
use sqlx::PgPool;

use support::llm::{API_KEY, MASTER_KEY, SCRIPTED_CALL_MICROS, ask};
use support::refusals::latest_entry;
use support::{Client, TestApi, owner_url};

async fn owner() -> PgPool {
    PgPool::connect(owner_url()).await.unwrap()
}

fn formula(formula: SupplyFormula, cap: CapChoice) -> SupplyChange {
    SupplyChange::Formula { formula, cap }
}

/// The demo company changed by the operator.
async fn supply(owner: &PgPool, change: SupplyChange) -> anyhow::Result<()> {
    change_model_supply(owner, tenant::SLUG, change)
        .await
        .map(|_| ())
}

/// The demo company as the seed left it: capped at the default, no markup.
async fn restore(owner: &PgPool) {
    supply(owner, formula(SupplyFormula::Capped, CapChoice::Default))
        .await
        .unwrap();
    supply(owner, SupplyChange::Markup { bps: 0 })
        .await
        .unwrap();
}

fn refusal(result: anyhow::Result<()>) -> String {
    let error = result.unwrap_err();
    error
        .downcast_ref::<DomainError>()
        .unwrap_or_else(|| panic!("not a refusal: {error:#}"))
        .message
        .clone()
}

/// The company's audit entries after `since`: action, actor and metadata.
async fn audited(api: &TestApi, since: i64) -> Vec<(String, String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select action, actor_type, metadata from audit_log \
         where id > $1 and action like 'model_supply.%' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_operator_supplies_a_company_each_change_traced_by_the_system() {
    let api = TestApi::start().await;
    let owner = owner().await;
    let since = latest_entry(&api).await;
    // The register keeps what the other tests recorded: counted from here.
    let start = model_supply_report(&owner, tenant::SLUG).await.unwrap();

    supply(&owner, formula(SupplyFormula::Prepaid, CapChoice::Default))
        .await
        .unwrap();
    for (amount_micros, reference) in [(500_000_000, "FAC-2026-0042"), (-20_000_000, "AV-2026-1")] {
        let credit = SupplyChange::Credit {
            amount_micros,
            reference: reference.into(),
        };
        supply(&owner, credit).await.unwrap();
    }
    supply(&owner, SupplyChange::Markup { bps: 1_500 })
        .await
        .unwrap();
    let report = model_supply_report(&owner, tenant::SLUG).await.unwrap();
    assert_eq!(report.formula, SupplyFormula::Prepaid);
    assert_eq!(report.markup_bps, 1_500);
    let credited = report.spending.credited_micros - start.spending.credited_micros;
    assert_eq!(credited, 480_000_000);
    let balance = report.spending.balance_micros() - start.spending.balance_micros();
    assert_eq!(balance, 480_000_000);
    let references: Vec<&str> = report.entries[start.entries.len()..]
        .iter()
        .map(|e| e.reference.as_str())
        .collect();
    assert_eq!(references, ["FAC-2026-0042", "AV-2026-1"]);
    // Without a cap: only when asked explicitly, and said.
    supply(&owner, formula(SupplyFormula::Capped, CapChoice::Unlimited))
        .await
        .unwrap();
    let report = model_supply_report(&owner, tenant::SLUG).await.unwrap();
    assert!(report.unlimited && report.monthly_cap_micros.is_none());

    for (change, said) in [
        (
            formula(SupplyFormula::Prepaid, CapChoice::Amount(50_000_000)),
            "qu’avec la formule capped",
        ),
        (
            formula(SupplyFormula::Capped, CapChoice::Amount(500_000)),
            "1 à 100 000 USD",
        ),
        (SupplyChange::Markup { bps: 10_001 }, "0 à 100 %"),
        (
            SupplyChange::Credit {
                amount_micros: 0,
                reference: "FAC-0".into(),
            },
            "Un versement",
        ),
        (
            SupplyChange::Credit {
                amount_micros: 1_000_000,
                reference: "FAC-2026-0042".into(),
            },
            "déjà cette référence",
        ),
    ] {
        assert!(
            refusal(supply(&owner, change).await).contains(said),
            "{said}"
        );
    }
    let unknown = change_model_supply(&owner, "inconnue", SupplyChange::Markup { bps: 0 }).await;
    assert!(refusal(unknown.map(|_| ())).contains("Aucune entreprise"));

    let entries = audited(&api, since).await;
    let actions: Vec<&str> = entries
        .iter()
        .map(|(action, _, _)| action.as_str())
        .collect();
    assert_eq!(
        actions,
        [
            "model_supply.configured",
            "model_supply.credit_added",
            "model_supply.credit_added",
            "model_supply.markup_set",
            "model_supply.configured",
        ]
    );
    assert!(entries.iter().all(|(_, actor, _)| actor == "system"));
    assert_eq!(entries[0].2["formula"], "prepaid");
    assert_eq!(entries[0].2["previousFormula"], "capped");
    assert_eq!(entries[2].2["amountMicros"], -20_000_000);
    assert_eq!(entries[4].2["unlimited"], true);
    restore(&owner).await;
}

/// Waits until the company's assistant is available or not, as `client`
/// reads it, once every process loaded the operator's change.
async fn assistant(client: &Client<'_>, available: bool) {
    for _ in 0..100 {
        let me = client.get("/auth/me").await.json();
        if me["assistant"]["available"] == available {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the assistant never became available: {available}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_the_operator_does_not_supply_has_no_platform_model() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = owner().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    assert!(paul.get("/admin/llm").await.json()["platform"].is_object());

    supply(
        &owner,
        formula(SupplyFormula::ClientKey, CapChoice::Default),
    )
    .await
    .unwrap();
    assistant(&marc, false).await;
    assert_eq!(paul.get("/admin/llm").await.json()["platform"], Value::Null);
    let (status, _) = ask(&api, &marc, "Bonjour").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(api.model.as_ref().unwrap().requests().is_empty());

    // Its own key serves it; removed, nothing is left: the operator's key
    // is never its to take.
    let own = json!({ "provider": "anthropic", "model": "claude-haiku-5-5", "apiKey": API_KEY });
    assert_eq!(paul.put("/admin/llm", own).await.status, StatusCode::OK);
    assistant(&marc, true).await;
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
    assistant(&marc, false).await;

    supply(&owner, formula(SupplyFormula::Capped, CapChoice::Default))
        .await
        .unwrap();
    assistant(&marc, true).await;
    restore(&owner).await;
}

/// Waits until this process loaded the operator's change of the formula
/// and of the markup (announced to every process).
async fn reloaded(api: &TestApi, formula: SupplyFormula, markup_bps: u32) {
    for _ in 0..100 {
        let supply = api
            .app
            .services
            .llm
            .resolve(tenant::ID)
            .await
            .unwrap()
            .supply;
        if (supply.formula, supply.markup_bps) == (formula, markup_bps) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the company's supply was never loaded again");
}

/// The last model call recorded: its formula, real cost and re-invoiced
/// amount.
async fn charged(api: &TestApi) -> Vec<(String, i64, Option<i64>)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select supply_formula, cost_micros, billed_micros from usage_records \
         where kind = 'llm' order by id desc limit 1",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_call_carries_its_formula_and_on_the_operator_s_key_its_re_invoiced_amount() {
    // Each call of the scripted model costs `SCRIPTED_CALL_MICROS`.
    let api = TestApi::start_priced_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = owner().await;
    let marc = api.sign_in(user::MARC).await;
    supply(&owner, SupplyChange::Markup { bps: 1_500 })
        .await
        .unwrap();
    reloaded(&api, SupplyFormula::Capped, 1_500).await;
    let billed = SCRIPTED_CALL_MICROS * 115 / 100;

    api.script(vec![ScriptedStep::text("Sur le plafond.")]);
    ask(&api, &marc, "Bonjour").await;
    assert_eq!(
        charged(&api).await,
        [("capped".into(), SCRIPTED_CALL_MICROS, Some(billed))]
    );

    supply(&owner, formula(SupplyFormula::Prepaid, CapChoice::Default))
        .await
        .unwrap();
    // The register keeps what the other tests recorded: counted from here.
    let start = model_supply_report(&owner, tenant::SLUG).await.unwrap();
    let credit = SupplyChange::Credit {
        amount_micros: 10_000_000,
        reference: "FAC-PREPAYE".into(),
    };
    supply(&owner, credit).await.unwrap();
    reloaded(&api, SupplyFormula::Prepaid, 1_500).await;
    api.script(vec![ScriptedStep::text("Sur le crédit.")]);
    ask(&api, &marc, "Bonjour").await;
    assert_eq!(
        charged(&api).await,
        [("prepaid".into(), SCRIPTED_CALL_MICROS, Some(billed))]
    );
    let report = model_supply_report(&owner, tenant::SLUG).await.unwrap();
    let balance = report.spending.balance_micros() - start.spending.balance_micros();
    assert_eq!(balance, 10_000_000 - billed);
    let cost = report.spending.prepaid_cost_micros - start.spending.prepaid_cost_micros;
    assert_eq!(cost, SCRIPTED_CALL_MICROS);

    // On the company's own key, nothing is re-invoiced.
    let paul = api.sign_in(user::PAUL).await;
    let own = json!({ "provider": "anthropic", "model": "claude-haiku-5-5", "apiKey": API_KEY });
    assert_eq!(paul.put("/admin/llm", own).await.status, StatusCode::OK);
    api.company_model
        .push([ScriptedStep::text("Sur la clé de l’entreprise.")]);
    ask(&api, &marc, "Bonjour").await;
    assert_eq!(
        charged(&api).await,
        [("client_key".into(), SCRIPTED_CALL_MICROS, None)]
    );
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
    restore(&owner).await;
}
