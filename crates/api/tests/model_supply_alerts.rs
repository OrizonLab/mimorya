//! The alerts of the operator's supply (ADR-211), by the periodic pass: the
//! administrators of a company's settings told by e-mail once its month
//! comes to a share of its cap, or its credit's balance falls to a share of
//! its last payment; the most pressing share alone, once for its month and
//! cap or its payment, a cap raised or a new payment re-arming them;
//! audited without the address. The shares are the operator's, set by
//! `mimorya-admin` within bounds and traced. Nothing is sent nor recorded
//! without a relay. Who is told, and when the pass runs:
//! `model_supply_alert_recipients.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::provisioning::{
    CapChoice, SupplyChange, change_model_supply, model_supply_report,
};
use mimorya_api::services::supply_alerts::{SUPPLY_ALERTED, send_supply_alerts};
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::SupplyFormula;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::model_supply::{CAP, capped, credit_left, spent, supply};
use support::refusals::{entries_since, latest_entry};
use support::supply_alerts::{alerts, leave, pass, recorded};
use support::{TestApi, email, owner_url};

const EIGHTY: &str = "Consommation du modèle : 80 % du plafond mensuel atteints";
const NINETY_FIVE: &str = "Consommation du modèle : 95 % du plafond mensuel atteints";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_shares_of_the_cap_are_told_once_each_for_the_month_and_its_cap() {
    let api = TestApi::start_with_mailer().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    leave(&owner).await;
    supply(&owner, capped(CapChoice::Amount(CAP))).await;
    let since = latest_entry(&api).await;

    spent(&owner, SupplyFormula::Capped, CAP * 79 / 100).await;
    assert!(pass(&api).await.is_empty());
    spent(&owner, SupplyFormula::Capped, CAP * 80 / 100).await;
    assert_eq!(pass(&api).await, [EIGHTY]);
    // Once for the month.
    assert!(pass(&api).await.is_empty());
    // Past 95 % at once: that share alone.
    spent(&owner, SupplyFormula::Capped, CAP).await;
    assert_eq!(pass(&api).await, [NINETY_FIVE]);
    // The operator adds the cap itself: told once reached.
    supply(&owner, alerts(Some(vec![80, 95, 100]), None)).await;
    let reached = "Plafond mensuel de consommation du modèle atteint";
    assert_eq!(pass(&api).await, [reached]);
    assert!(pass(&api).await.is_empty());

    // The cap raised, as the e-mail asks of whom may: its shares told anew.
    supply(&owner, capped(CapChoice::Amount(3 * CAP))).await;
    assert!(pass(&api).await.is_empty());
    spent(&owner, SupplyFormula::Capped, 3 * CAP * 80 / 100).await;
    assert_eq!(pass(&api).await, [EIGHTY]);
    spent(&owner, SupplyFormula::Capped, 3 * CAP * 95 / 100).await;
    assert_eq!(pass(&api).await, [NINETY_FIVE]);

    let audit = entries_since(&api, since).await;
    assert_eq!(audit.matches(SUPPLY_ALERTED).count(), 5, "{audit}");
    assert!(audit.contains("model_supply.alerts_set"), "{audit}");
    assert!(!audit.contains(&email(user::PAUL)), "{audit}");
    leave(&owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_shares_of_the_last_payment_are_told_again_after_a_new_one() {
    let api = TestApi::start_with_mailer().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    leave(&owner).await;
    let prepaid = SupplyChange::Formula {
        formula: SupplyFormula::Prepaid,
        cap: CapChoice::Default,
    };
    supply(&owner, prepaid).await;
    let paid = |reference: &str| SupplyChange::Credit {
        amount_micros: 10_000_000,
        reference: reference.into(),
    };
    supply(&owner, paid("FAC-2026-1101")).await;

    credit_left(&owner, 2_000_001).await;
    assert!(pass(&api).await.is_empty());
    credit_left(&owner, 2_000_000).await;
    let twenty = "Crédit de consommation du modèle : solde sous 20 % du dernier versement";
    assert_eq!(pass(&api).await, [twenty]);
    assert!(pass(&api).await.is_empty());
    credit_left(&owner, 400_000).await;
    let five = "Crédit de consommation du modèle : solde sous 5 % du dernier versement";
    assert_eq!(pass(&api).await, [five]);

    // A new payment: its shares are told again, once reached.
    supply(&owner, paid("FAC-2026-1102")).await;
    credit_left(&owner, 3_000_000).await;
    assert!(pass(&api).await.is_empty());
    credit_left(&owner, 1_500_000).await;
    assert_eq!(pass(&api).await, [twenty]);
    leave(&owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_relay_nothing_is_sent_nor_recorded() {
    let api = TestApi::start().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    leave(&owner).await;
    supply(&owner, capped(CapChoice::Amount(CAP))).await;
    spent(&owner, SupplyFormula::Capped, CAP).await;
    api.app.services.runs.start_executing().unwrap();
    assert_eq!(send_supply_alerts(&api.app.services).await.unwrap(), 0);
    assert_eq!(recorded(&api).await, 0);
    leave(&owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_operator_sets_the_shares_within_their_bounds_and_the_company_reads_them() {
    let api = TestApi::start().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    leave(&owner).await;
    let paul = api.sign_in(user::PAUL).await;
    let shown = paul.get("/admin/model-supply").await.json();
    assert_eq!(
        (&shown["capAlertPercents"], &shown["creditAlertPercents"]),
        (&json!([80, 95]), &json!([5, 20]))
    );
    for refused in [
        alerts(Some(vec![0]), None),
        alerts(Some(vec![101]), None),
        alerts(None, Some(vec![100])),
        alerts(Some(vec![80, 80]), None),
        alerts(Some(vec![10, 20, 30, 40, 50, 60]), None),
    ] {
        let said = change_model_supply(&owner, tenant::SLUG, refused.clone()).await;
        assert!(said.is_err(), "{refused:?}");
    }
    supply(&owner, alerts(None, Some(vec![30, 0]))).await;
    let report = model_supply_report(&owner, tenant::SLUG).await.unwrap();
    assert_eq!(
        (report.cap_alerts, report.credit_alerts),
        (vec![80, 95], vec![0, 30])
    );
    let shown = paul.get("/admin/model-supply").await;
    assert_eq!(shown.status, StatusCode::OK);
    let read: Value = shown.json();
    assert_eq!(read["creditAlertPercents"], json!([0, 30]));
    leave(&owner).await;
}
