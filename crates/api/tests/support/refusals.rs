//! What the audit says of refused administration changes (ADR-038), of
//! those aimed at a resource answered as absent (ADR-047) and of those
//! sending an identifier that designates nothing (ADR-054), read with the
//! application role.

use mimorya_api::plugins::refusal_audit::{
    NOT_FOUND_ACTION, REFUSED_ACTION, UNKNOWN_REFERENCE_ACTION,
};
use mimorya_core::demo::tenant;
use serde_json::Value;
use uuid::Uuid;

use super::TestApi;

#[derive(Debug)]
pub struct Refused {
    pub resource: Option<(String, String)>,
    pub reason: Option<String>,
    pub metadata: Value,
    pub ip: Option<String>,
    pub request_id: Option<String>,
}

#[derive(sqlx::FromRow)]
struct RefusalRow {
    actor_user_id: Option<Uuid>,
    outcome: String,
    resource_type: Option<String>,
    resource_id: Option<String>,
    reason: Option<String>,
    metadata: Value,
    ip: Option<String>,
    request_id: Option<String>,
}

/// The id of the latest audit entry, before the requests a test checks.
pub async fn latest_entry(api: &TestApi) -> i64 {
    latest_entry_of(api, tenant::ID).await
}

/// The id of the latest audit entry of a company.
pub async fn latest_entry_of(api: &TestApi, tenant_id: Uuid) -> i64 {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query_scalar("select coalesce(max(id), 0) from audit_log")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

/// The refusals audited after `since`, in order, for every user.
pub async fn refusals_since(api: &TestApi, since: i64) -> Vec<(Uuid, Refused)> {
    denied_since(api, REFUSED_ACTION, since).await
}

/// The changes on resources answered as absent audited after `since`.
pub async fn not_found_since(api: &TestApi, since: i64) -> Vec<(Uuid, Refused)> {
    denied_since(api, NOT_FOUND_ACTION, since).await
}

/// The changes sending an unknown identifier audited after `since`.
pub async fn unknown_references_since(api: &TestApi, since: i64) -> Vec<(Uuid, Refused)> {
    denied_since(api, UNKNOWN_REFERENCE_ACTION, since).await
}

async fn denied_since(api: &TestApi, action: &str, since: i64) -> Vec<(Uuid, Refused)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<RefusalRow> = sqlx::query_as(
        "select actor_user_id, outcome, resource_type, resource_id, reason, metadata, \
         host(ip) as ip, request_id from audit_log where action = $1 and id > $2 order by id",
    )
    .bind(action)
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows.into_iter()
        .map(|row| {
            assert_eq!(row.outcome, "denied");
            let refused = Refused {
                resource: row.resource_type.zip(row.resource_id),
                reason: row.reason,
                metadata: row.metadata,
                ip: row.ip,
                request_id: row.request_id,
            };
            (row.actor_user_id.expect("an actor"), refused)
        })
        .collect()
}

/// Every entry written after `since`, as text.
pub async fn entries_since(api: &TestApi, since: i64) -> String {
    entries_of_since(api, tenant::ID, since).await
}

/// Every entry of a company written after `since`, as text.
pub async fn entries_of_since(api: &TestApi, tenant_id: Uuid, since: i64) -> String {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let rows: Vec<Value> =
        sqlx::query_scalar("select to_jsonb(audit_log) from audit_log where id > $1 order by id")
            .bind(since)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    Value::from(rows).to_string()
}
