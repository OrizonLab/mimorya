//! SCIM as the tests reach it (ADR-183): tokens made and removed by an
//! administrator, and the identity provider's requests, with its token, its
//! media type and without the header the browser's changes carry.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};

use super::{Client, TestApi, TestResponse};

pub const TOKENS: &str = "/admin/scim/tokens";
pub const SCIM_JSON: &str = "application/scim+json";

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

/// What the provider holding `token` gets from `method` at `/scim/v2{path}`,
/// sending `body` as `content_type`.
pub async fn scim_as(
    api: &TestApi,
    token: Option<&str>,
    method: Method,
    path: &str,
    body: Option<(&str, String)>,
) -> TestResponse {
    let mut request = Request::builder()
        .method(method)
        .uri(format!("/scim/v2{path}"));
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let body = match body {
        Some((content_type, body)) => {
            request = request.header("content-type", content_type);
            Body::from(body)
        }
        None => Body::empty(),
    };
    api.send(request.body(body).unwrap()).await
}

/// `GET /scim/v2{path}` with `token`.
pub async fn get(api: &TestApi, token: Option<&str>, path: &str) -> TestResponse {
    scim_as(api, token, Method::GET, path, None).await
}

/// `POST /scim/v2/Users` of `user` with `token`, as SCIM sends it.
pub async fn create(api: &TestApi, token: Option<&str>, user: &Value) -> TestResponse {
    let body = Some((SCIM_JSON, user.to_string()));
    scim_as(api, token, Method::POST, "/Users", body).await
}

/// How many accounts the provider holding `secret` sees.
pub async fn total(api: &TestApi, secret: &str) -> i64 {
    let listed = get(api, Some(secret), "/Users?count=0").await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    listed.json()["totalResults"].as_i64().unwrap()
}

/// When the token `id` was last used, as the administration lists it.
pub async fn last_used(client: &Client<'_>, id: &str) -> Value {
    let listed = client.get(TOKENS).await.json();
    let tokens = listed["tokens"].as_array().unwrap().clone();
    let token = tokens.into_iter().find(|t| t["id"] == json!(id));
    token.expect("token listed")["lastUsedAt"].clone()
}

/// `method` at `/scim/v2{path}` with `token`, sending `body` as SCIM does.
pub async fn send(
    api: &TestApi,
    token: Option<&str>,
    method: Method,
    path: &str,
    body: &Value,
) -> TestResponse {
    scim_as(
        api,
        token,
        method,
        path,
        Some((SCIM_JSON, body.to_string())),
    )
    .await
}

/// A patch of `operations`, as Entra ID sends it.
pub fn patch_of(operations: Value) -> Value {
    json!({
        "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
        "Operations": operations
    })
}

/// An account as Entra ID sends it, at `user_name`.
pub fn entra_user(user_name: &str, external_id: &str) -> Value {
    json!({
        "schemas": [
            "urn:ietf:params:scim:schemas:core:2.0:User",
            "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User"
        ],
        "externalId": external_id,
        "userName": user_name,
        "active": true,
        "displayName": "Inès Garnier",
        "emails": [{ "primary": true, "type": "work", "value": user_name }],
        "name": { "formatted": "Inès Garnier", "familyName": "Garnier", "givenName": "Inès" },
        "title": "Technicienne",
        "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User": { "department": "Lyon" }
    })
}

/// The SCIM refusal `response` holds: its status and its `scimType`.
pub fn refusal(response: &TestResponse) -> (StatusCode, Value) {
    let body = response.json();
    assert_eq!(
        body["schemas"],
        json!(["urn:ietf:params:scim:api:messages:2.0:Error"]),
        "{}",
        response.text()
    );
    assert_eq!(body["status"], json!(response.status.as_u16().to_string()));
    (response.status, body["scimType"].clone())
}
