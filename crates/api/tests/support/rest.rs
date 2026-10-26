//! A REST API a company adds (ADR-112): the test API on a local port
//! (`mimorya_connectors::testing::serve_rest_http`, reachable when the
//! operator allows the private network), described with two operations.

use axum::http::StatusCode;
use mimorya_connectors::testing::KEY_HEADER;
use serde_json::{Value, json};

use super::Client;

pub const KEY: &str = "cle-api-de-test-0123456789";
pub const READ: &str = "rest_crm_get_item";
pub const CREATE: &str = "rest_crm_create_item";

/// The operations: one reading an item, one creating one.
pub fn operations() -> Value {
    json!([
        {
            "name": "get_item",
            "title": "Lire un élément",
            "description": "Renvoie l’élément demandé.",
            "method": "GET",
            "path": "/echo/items/{id}",
            "parameters": [{ "name": "id", "in": "path", "type": "string" }],
        },
        {
            "name": "create_item",
            "title": "Créer un élément",
            "description": "Crée un élément.",
            "method": "POST",
            "path": "/echo/items",
            "parameters": [{ "name": "title", "in": "body", "type": "string", "required": true }],
        },
    ])
}

/// The API at `url`, prefixed `crm`, its key sent in `X-Api-Key`.
pub fn rest_body(name: &str, url: &str, key: Option<&str>) -> Value {
    let mut body = json!({
        "name": name,
        "prefix": "crm",
        "url": url,
        "tokenHeader": KEY_HEADER,
        "operations": operations(),
    });
    if let Some(key) = key {
        body["token"] = json!(key);
    }
    body
}

/// Adds the API and returns its id.
pub async fn add_rest(admin: &Client<'_>, body: Value) -> String {
    let created = admin.post("/admin/connectors/rest", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_string()
}
