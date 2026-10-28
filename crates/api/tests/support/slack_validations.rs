//! A validation given in Slack in the tests (ADR-129): the buttons of a
//! pause's message, and a press of one of them as Slack sends it, signed,
//! form-encoded, to the gateway's address.

use axum::body::Body;
use axum::http::Request;
use mimorya_connectors::testing::{SlackPost, WORKSPACE, signed};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{TestApi, TestResponse};

/// The approval whose buttons come first in `post`.
pub fn buttons_of(post: &SlackPost) -> Uuid {
    let blocks = post.blocks.as_ref().expect("its blocks");
    let value = blocks
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["type"] == "actions")
        .map(|b| b["elements"][0]["value"].as_str().unwrap())
        .expect("its buttons");
    value.parse().unwrap()
}

/// What Slack sends when `person` of `workspace` presses the « Valider »
/// (`approved`) or « Refuser » button of `approval` in `post`.
pub fn pressed(
    workspace: &str,
    person: &str,
    post: &SlackPost,
    approval: Uuid,
    approved: bool,
) -> String {
    let action = if approved {
        "approval_approve"
    } else {
        "approval_reject"
    };
    let payload = json!({
        "type": "block_actions",
        "team": { "id": workspace },
        "user": { "id": person },
        "container": {
            "type": "message",
            "channel_id": post.channel,
            "message_ts": post.ts,
            "thread_ts": post.thread_ts,
        },
        "message": { "blocks": post.blocks.clone().unwrap_or(Value::Null) },
        "actions": [{ "action_id": action, "value": approval.to_string() }],
    });
    let encoded: String =
        url::form_urlencoded::byte_serialize(payload.to_string().as_bytes()).collect();
    format!("payload={encoded}")
}

/// What `person` of the company's workspace pressing it gets.
pub fn pressed_by(person: &str, post: &SlackPost, approved: bool) -> String {
    pressed(WORKSPACE, person, post, buttons_of(post), approved)
}

/// Posts `body` to the gateway `id` as Slack sends a button pressed:
/// form-encoded, signed now unless `unsigned`.
pub async fn press(api: &TestApi, id: Uuid, body: &str, unsigned: bool) -> TestResponse {
    let mut request = Request::post(format!("/gateways/slack/{id}"))
        .header("content-type", "application/x-www-form-urlencoded");
    if !unsigned {
        let (timestamp, signature) = signed(body);
        request = request
            .header("x-slack-request-timestamp", timestamp)
            .header("x-slack-signature", signature);
    }
    api.send(request.body(Body::from(body.to_owned())).unwrap())
        .await
}
