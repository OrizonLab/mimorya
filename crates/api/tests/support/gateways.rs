//! The company's Slack gateway in the tests (ADR-125): declared at the local
//! Slack, what Slack sends it, signed with its secret, and the link it
//! offers read back from the message it posted.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mimorya_connectors::testing::{BOT_TOKEN, FakeSlack, SIGNING_SECRET, WORKSPACE, signed};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, TestApi, TestResponse};

pub const GATEWAYS: &str = "/admin/gateways";
pub const DECLARE: &str = "/admin/gateways/slack";
pub const ACCOUNT: &str = "/account/gateways";
pub const LINK: &str = "/account/gateways/link";
pub const PREVIEW: &str = "/account/gateways/link/preview";

/// The company's Slack gateway, declared by `admin` with the local Slack's
/// application; one a previous test declared goes first, with its people's
/// identities.
pub async fn declare_slack(admin: &Client<'_>) -> Uuid {
    remove_gateways(admin).await;
    let declared = admin
        .post(
            DECLARE,
            json!({
                "name": "Slack Néo",
                "botToken": BOT_TOKEN,
                "signingSecret": SIGNING_SECRET,
            }),
        )
        .await;
    assert_eq!(declared.status, StatusCode::CREATED, "{}", declared.text());
    declared.json()["id"].as_str().unwrap().parse().unwrap()
}

/// Deletes the company's gateways.
pub async fn remove_gateways(admin: &Client<'_>) {
    let listed = admin.get(GATEWAYS).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    for gateway in listed.json()["gateways"].as_array().unwrap() {
        let id = gateway["id"].as_str().unwrap();
        let deleted = admin.delete(&format!("{GATEWAYS}/{id}")).await;
        assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
    }
}

/// The gateway as the administration lists it.
pub async fn gateway_view(admin: &Client<'_>, id: Uuid) -> Value {
    let listed = admin.get(GATEWAYS).await.json();
    listed["gateways"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == id.to_string())
        .cloned()
        .unwrap_or(Value::Null)
}

/// Posts `body` to `path` as Slack would, with these signature headers,
/// without the browser's header.
pub async fn post_signed(
    api: &TestApi,
    path: &str,
    headers: Option<(&str, &str)>,
    body: &str,
) -> TestResponse {
    let mut request = Request::post(path).header("content-type", "application/json");
    if let Some((timestamp, signature)) = headers {
        request = request
            .header("x-slack-request-timestamp", timestamp)
            .header("x-slack-signature", signature);
    }
    api.send(request.body(Body::from(body.to_owned())).unwrap())
        .await
}

/// What Slack sending `body` to the gateway `id`, signed now, gets.
pub async fn send(api: &TestApi, id: Uuid, body: &str) -> TestResponse {
    let (timestamp, signature) = signed(body);
    post_signed(
        api,
        &format!("/gateways/slack/{id}"),
        Some((&timestamp, &signature)),
        body,
    )
    .await
}

/// A direct message `person` writes to the application, as Slack sends it.
pub fn direct_message(event_id: &str, person: &str, text: &str) -> String {
    envelope(
        WORKSPACE,
        event_id,
        json!({
            "type": "message",
            "channel": dm_of(person),
            "user": person,
            "text": text,
            "ts": "1700000100.000100",
            "channel_type": "im",
            "event_ts": "1700000100.000100"
        }),
    )
}

/// The direct conversation of the Slack user `person` with the
/// application, as the local Slack names it: `D…` after their `U…`.
pub fn dm_of(person: &str) -> String {
    format!("D{}", &person[1..])
}

/// An event of `workspace`, as Slack wraps it.
pub fn envelope(workspace: &str, event_id: &str, event: Value) -> String {
    json!({
        "token": "verification-token-unused",
        "team_id": workspace,
        "api_app_id": "A0MIMORYA",
        "event": event,
        "type": "event_callback",
        "event_id": event_id,
        "event_time": 1700000100
    })
    .to_string()
}

/// The code of the link the latest message posted offers.
pub fn offered_code(slack: &FakeSlack) -> String {
    let posts = slack.posts();
    let text = &posts.last().expect("a message posted").text;
    let start = text.find("gateway-link#").expect("a link") + "gateway-link#".len();
    let end = start + text[start..].find('|').expect("its label");
    text[start..end].to_owned()
}
