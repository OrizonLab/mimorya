//! The audit log's export as the tests reach it (ADR-181): a server that
//! seals, a SIEM's read with its token, tokens made and removed by an
//! administrator, the company's audit log as the administration reads it.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use super::{Client, TestApi, TestResponse, llm};

pub const TOKENS: &str = "/admin/audit/export-tokens";

/// A server with a master key: it seals, and exports.
pub async fn keyed() -> TestApi {
    TestApi::start_with(&[("MIMORYA_MASTER_KEY", llm::MASTER_KEY)]).await
}

/// What a SIEM holding `token` reads at `/audit/export{query}`.
pub async fn read(api: &TestApi, token: Option<&str>, query: &str) -> TestResponse {
    let mut request = Request::get(format!("/audit/export{query}"));
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    api.send(request.body(Body::empty()).unwrap()).await
}

/// A token named `name`: its identifier and its secret.
pub async fn made(client: &Client<'_>, name: &str) -> (String, String) {
    let created = client.post(TOKENS, json!({ "name": name })).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let body = created.json();
    let id = body["token"]["id"].as_str().unwrap().to_string();
    (id, body["secret"].as_str().unwrap().to_string())
}

pub async fn removed(client: &Client<'_>, id: &str) {
    let deleted = client.delete(&format!("{TOKENS}/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}

/// The company's audit log, newest first, as its administration reads it.
pub async fn audit_log(client: &Client<'_>) -> Vec<Value> {
    let log = client.get("/admin/audit?limit=100").await.json();
    log["entries"].as_array().unwrap().clone()
}

/// The seal the company `tenant_id`'s chain holds at the place `place`, as
/// the export writes it.
pub async fn seal_at(db: &mimorya_db::Database, tenant_id: uuid::Uuid, place: i64) -> String {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let seal = mimorya_db::repositories::audit_chain::audit_seal_at(&mut tx, place)
        .await
        .unwrap()
        .expect("a sealed place");
    mimorya_api::services::audit_chain::hex(&seal)
}

/// Waits until `count` sessions of the database wait for a lock: the
/// requests a test holds back have reached it. Five seconds at most: the test
/// fails then, having not played what it says.
pub async fn waiting_on_locks(db: &mimorya_db::Database, count: i64) {
    for _ in 0..250 {
        let waiting: i64 = sqlx::query_scalar(
            "select count(*) from pg_stat_activity \
             where datname = current_database() and wait_event_type = 'Lock'",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        if waiting >= count {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("fewer than {count} requests wait for a lock after five seconds");
}
