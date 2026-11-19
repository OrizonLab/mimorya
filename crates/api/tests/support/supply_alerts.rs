//! The alerts of the operator's supply (ADR-211) as the tests drive them: a
//! pass, as a process that executes runs makes every minute, what it sent
//! to whom, what it recorded, the operator's shares, and the company left
//! as the seed made it.

use mimorya_api::services::provisioning::{CapChoice, SupplyChange};
use mimorya_api::services::supply_alerts::send_supply_alerts;
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::SupplyFormula;
use sqlx::PgPool;

use super::model_supply::{capped, spent, supply};
use super::{TestApi, email};

/// A pass of the alerts in a process that executes runs: the subjects of
/// the e-mails it sent to Paul. None goes to Marc, who has no settings.
pub async fn pass(api: &TestApi) -> Vec<String> {
    api.app.services.runs.start_executing().unwrap();
    let before = api.mail.sent().len();
    send_supply_alerts(&api.app.services).await.unwrap();
    let sent = api.mail.sent()[before..].to_vec();
    assert!(
        sent.iter().all(|mail| mail.to != email(user::MARC)),
        "an account without the settings was told"
    );
    sent.into_iter()
        .filter(|mail| mail.to == email(user::PAUL))
        .map(|mail| mail.subject)
        .collect()
}

/// The addresses the last pass's e-mails went to.
pub fn told(api: &TestApi, before: usize) -> Vec<String> {
    api.mail.sent()[before..]
        .iter()
        .map(|mail| mail.to.clone())
        .collect()
}

pub fn alerts(cap: Option<Vec<u8>>, credit: Option<Vec<u8>>) -> SupplyChange {
    SupplyChange::Alerts { cap, credit }
}

/// The alerts recorded for the company.
pub async fn recorded(api: &TestApi) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar("select count(*) from tenant_supply_alerts")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

/// The company as the seed made it: nothing spent, the default shares and
/// cap, no alert recorded for the next tests.
pub async fn leave(owner: &PgPool) {
    spent(owner, SupplyFormula::Capped, 0).await;
    supply(owner, alerts(Some(vec![80, 95]), Some(vec![5, 20]))).await;
    supply(owner, capped(CapChoice::Default)).await;
    sqlx::query("delete from tenant_supply_alerts where tenant_id = $1")
        .bind(tenant::ID)
        .execute(owner)
        .await
        .unwrap();
}
