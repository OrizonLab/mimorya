//! An OpenAPI description read for the operations it offers (ADR-114):
//! only by connector administrators, nothing stored nor audited; what it
//! offers is a declaration the platform accepts as it comes; what cannot be
//! read is said.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::serve_rest_http;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::rest::add_rest;

const IMPORT: &str = "/admin/connectors/rest/import";

/// The company's audit entries.
async fn audited(api: &TestApi) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar("select count(*) from audit_log")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

/// The test API's two operations, as its description gives them.
fn description(url: &str) -> Value {
    let id =
        json!({ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } });
    let trace = json!({ "name": "X-Trace", "in": "header", "schema": { "type": "string" } });
    let item = json!({
        "type": "object",
        "required": ["title"],
        "properties": { "title": { "type": "string", "description": "Son titre." } },
    });
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Éléments", "version": "1" },
        "servers": [{ "url": url }],
        "paths": {
            "/echo/items/{id}": { "get": {
                "operationId": "getItem",
                "summary": "Lire un élément",
                "parameters": [id, trace],
            } },
            "/echo/items": { "post": {
                "operationId": "createItem",
                "summary": "Créer un élément",
                "requestBody": {
                    "required": true,
                    "content": { "application/json": { "schema": item } },
                },
            } },
        },
    })
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_description_offers_operations_to_declare() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let url = serve_rest_http().await;
    let body = json!({ "document": description(&url).to_string() });
    let forbidden = marc.post(IMPORT, body.clone()).await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);

    let before = audited(&api).await;
    let read = paul.post(IMPORT, body).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    let read = read.json();
    assert_eq!(read["url"], url);
    let names: Vec<&str> = read["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["get_item", "create_item"]);
    assert_eq!(read["operations"][1]["parameters"][0]["in"], "body");
    assert_eq!(
        read["notes"],
        json!(["GET /echo/items/{id} : paramètre « X-Trace » laissé de côté : un en-tête"])
    );
    // Read only: nothing stored, nothing audited.
    let connectors = paul.get("/admin/connectors").await.json();
    let instances = connectors["instances"].as_array().unwrap();
    assert!(
        instances.iter().all(|c| c["type"] != "rest"),
        "{connectors}"
    );
    assert_eq!(audited(&api).await, before);

    // As offered, a declaration the platform accepts.
    let added = json!({
        "name": "Éléments",
        "prefix": "items",
        "url": read["url"],
        "operations": read["operations"],
    });
    let id = add_rest(&paul, added).await;
    let stored = listed(&paul, &id).await;
    assert_eq!(stored["rest"]["operations"], read["operations"]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_cannot_be_read_is_said() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    for (document, expected) in [
        ("openapi: 3.1.0\npaths: {}", "un document JSON est attendu"),
        (r#"{ "swagger": "2.0" }"#, "Description Swagger 2.0"),
        (r#"{ "openapi": "3.0.0" }"#, "aucun chemin"),
    ] {
        let refused = paul.post(IMPORT, json!({ "document": document })).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{document}");
        let message = refused.error_message();
        assert!(message.contains(expected), "{document} : {message}");
    }
    let unknown = json!({ "document": "{}", "url": "https://api.example.com/openapi.json" });
    assert_eq!(
        paul.post(IMPORT, unknown).await.status,
        StatusCode::BAD_REQUEST
    );
}
