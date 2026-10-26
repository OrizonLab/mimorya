//! Inbound hooks (ADR-109): one written by its author, with its token, and
//! a system posting to it.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, TestApi, TestResponse};

pub const HOOKS: &str = "/inbound-hooks";
pub const INSTRUCTION: &str = "Résume la panne signalée et propose une intervention.";

/// A hook titled `title` written by `client`: its id and its token.
pub async fn hook(client: &Client<'_>, title: &str) -> (Uuid, String) {
    let made = client
        .post(HOOKS, json!({ "title": title, "instruction": INSTRUCTION }))
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.text());
    let made = made.json();
    let id = made["id"].as_str().unwrap().parse().unwrap();
    (id, made["token"].as_str().unwrap().to_owned())
}

/// The hook `id` as `client` lists it.
pub async fn listed(client: &Client<'_>, id: Uuid) -> Value {
    let list = client.get(HOOKS).await.json();
    list["hooks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hook| hook["id"] == id.to_string())
        .cloned()
        .unwrap_or(Value::Null)
}

/// What a system posting `body` to `path` with `authorization` gets, without
/// the header the browser's requests carry.
pub async fn post_to(
    api: &TestApi,
    path: &str,
    authorization: Option<&str>,
    body: impl Into<Body>,
) -> TestResponse {
    let mut request = Request::post(path).header("content-type", "application/json");
    if let Some(authorization) = authorization {
        request = request.header("authorization", authorization);
    }
    api.send(request.body(body.into()).unwrap()).await
}

/// What a system posting `body` to the hook `id` with `token` gets.
pub async fn deliver(api: &TestApi, id: Uuid, token: &str, body: &str) -> TestResponse {
    let bearer = format!("Bearer {token}");
    post_to(api, &format!("/hooks/{id}"), Some(&bearer), body.to_owned()).await
}
