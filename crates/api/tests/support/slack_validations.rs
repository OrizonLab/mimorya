//! A validation given in Slack in the tests (ADR-129): a request written
//! there paused for its author's validation, the buttons of the pause's
//! message, and a press of one of them as Slack sends it, signed,
//! form-encoded, to the gateway's address.

use axum::body::Body;
use axum::http::Request;
use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::{FakeSlack, SlackPost, WORKSPACE, signed};
use serde_json::{Value, json};
use uuid::Uuid;

use super::planning::{apply_proposal, propose};
use super::slack_conversations::{answer, slack_runs, write, written};
use super::{TestApi, TestResponse};

/// The thread the request is written in.
pub const ASKED_IN: &str = "1700000200.000100";

/// The messages written in the thread `thread`, oldest first.
pub fn posts_in(slack: &FakeSlack, thread: &str) -> Vec<SlackPost> {
    (slack.posts().into_iter())
        .filter(|p| p.thread_ts.as_deref() == Some(thread))
        .collect()
}

/// The request `person` writes to plan INT-2052, outside any thread,
/// paused for its validation: its run.
pub async fn asked(api: &TestApi, (gateway, person): (Uuid, &str), end: &str) -> String {
    api.script(vec![
        propose("INT-2052", "15:00"),
        apply_proposal(""),
        ScriptedStep::text(end),
    ]);
    let message = written(person, "Planifie INT-2052", ASKED_IN, None);
    write(api, gateway, &message).await;
    slack_runs(api).await.remove(0).0.to_string()
}

/// The same, the pause written in its thread: its run, and that message.
pub async fn paused(
    api: &TestApi,
    slack: &FakeSlack,
    asker: (Uuid, &str),
    end: &str,
) -> (String, SlackPost) {
    let run = asked(api, asker, end).await;
    assert_eq!(answer(api).await, 1);
    (run, posts_in(slack, ASKED_IN).remove(0))
}

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

/// `post` as if it were in the direct conversation of `person`, as a press
/// may claim.
pub fn moved_to(post: &SlackPost, person: &str) -> SlackPost {
    let mut moved = post.clone();
    moved.channel = super::gateways::dm_of(person);
    moved
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
