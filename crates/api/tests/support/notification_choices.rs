//! The e-mails one chooses to receive (ADR-094): one's choice, made through
//! the API, and what became of the notices of a request, read in the audit.

use axum::http::StatusCode;
use mimorya_core::demo::tenant;
use serde_json::{Value, json};

use super::{Client, TestApi};

pub const NOTIFICATIONS: &str = "/account/notifications";

/// The choice, as the API reads and takes it.
pub fn chosen(approvals: bool, own_runs: bool, mission_ends: bool) -> Value {
    json!({ "approvals": approvals, "ownRuns": own_runs, "missionEnds": mission_ends })
}

/// Has `client` choose `choice`.
pub async fn choose(client: &Client<'_>, choice: Value) {
    let set = client.put(NOTIFICATIONS, choice).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
}

/// The audited outcomes and metadata of `action` for `approval`.
pub async fn audited(api: &TestApi, action: &str, approval: &Value) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select outcome, metadata from audit_log where action = $1 and resource_id = $2 \
         order by id",
    )
    .bind(action)
    .bind(approval["id"].as_str().unwrap())
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}
