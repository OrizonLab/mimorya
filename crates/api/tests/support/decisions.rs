//! A company's decision service as its tests set it (ADR-213): the app with
//! Jev opened by the operator or not, TypeSafe's API a local one; the
//! settings sent, every usage off; and the calls recorded with the role
//! « décision ».

use axum::http::StatusCode;
use mimorya_connectors::testing::FakeJev;
use mimorya_core::demo::tenant;
use serde_json::{Value, json};

use super::llm::MASTER_KEY;
use super::{Client, Setup, TestApi, owner_url};

pub const DECISIONS: &str = "/admin/decisions";
pub const USAGES: [&str; 7] = [
    "modelRouting",
    "skillChoice",
    "memoryGuard",
    "documentLabels",
    "actionGuard",
    "injectionDetection",
    "mailTriage",
];

impl TestApi {
    /// The app able to keep a company's key, priced, Jev asked at the local
    /// `jev`, opened by the operator when `opened`.
    pub async fn start_with_jev(jev: &FakeJev, opened: bool) -> Self {
        // A silent Jev given up on quickly.
        Self::start_with_jev_waiting(jev, opened, "300").await
    }

    /// The same, Jev given up on after `millis`.
    pub async fn start_with_jev_waiting(jev: &FakeJev, opened: bool, millis: &str) -> Self {
        let opened = if opened { "true" } else { "false" };
        Self::scripted(Setup {
            env: &[
                ("MIMORYA_MASTER_KEY", MASTER_KEY),
                ("MIMORYA_TYPESAFE_JEV", opened),
                ("MIMORYA_DECISION_TIMEOUT_MS", millis),
            ],
            typesafe_api: Some(jev.url.clone()),
            priced: true,
            ..Setup::default()
        })
        .await
    }
}

/// `engine`, with `key` if any, every usage off at 0.5.
pub fn settings(engine: &str, key: Option<&str>) -> Value {
    let usages: Vec<Value> = USAGES
        .iter()
        .map(|usage| json!({ "key": usage, "mode": "off", "threshold": 0.5 }))
        .collect();
    let mut body = json!({ "engine": engine, "usages": usages });
    if let Some(key) = key {
        body["jevApiKey"] = json!(key);
    }
    body
}

/// The main model's answer of « oui », with `certainty`, to the test's question.
pub fn main_model_yes(certainty: &str) -> String {
    json!({ "salutation": { "reponse": true, "certitude": certainty } }).to_string()
}

/// `body` refused with `status`, saying `said`.
pub async fn refused(paul: &Client<'_>, body: Value, status: StatusCode, said: &str) {
    let refused = paul.put(DECISIONS, body.clone()).await;
    assert_eq!(refused.status, status, "{body}: {}", refused.text());
    assert!(
        refused.error_message().contains(said),
        "{body}: {}",
        refused.text()
    );
}

/// `body` accepted.
pub async fn set(paul: &Client<'_>, body: Value) {
    let set = paul.put(DECISIONS, body.clone()).await;
    assert_eq!(set.status, StatusCode::OK, "{body}: {}", set.text());
}

/// The company's calls with the role « décision »: provider, model, formula,
/// cost and amount re-invoiced.
pub async fn decision_calls(api: &TestApi) -> Vec<(String, String, String, i64, Option<i64>)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let calls = sqlx::query_as(
        "select provider, model, supply_formula, cost_micros, billed_micros \
         from usage_records where role = 'decision' order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    calls
}

/// Forgets the calls of the binary's tests before, as the database's owner.
pub async fn forget_decision_calls() {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("delete from usage_records where role = 'decision'")
        .execute(&owner)
        .await
        .unwrap();
}
