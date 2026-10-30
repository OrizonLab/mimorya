//! What the end-to-end tests ask the test Microsoft from outside
//! (ADR-139): the messages the bots wrote, and the activity a person's
//! message to a bot would be, as the Bot Framework sends it, with its
//! token, for them to send the bot's messaging endpoint.

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::State;
use mimorya_core::gateways::teams::BOT_FRAMEWORK_ISSUER;
use serde::Deserialize;
use serde_json::{Value, json};

use super::signing::{Signer, signed_token};
use super::{Shared, TEAMS_DIRECTORY, conversation_of, member_id};

type SharedState = State<Arc<Mutex<Shared>>>;

/// The claims of a token the Bot Framework sends the bot `app` with an
/// activity of the service at `service_url`, valid now.
pub(super) fn claims(service_url: &str, app: &str) -> Value {
    let now = chrono::Utc::now().timestamp();
    json!({
        "iss": BOT_FRAMEWORK_ISSUER,
        "aud": app,
        "exp": now + 3600,
        "nbf": now - 60,
        "serviceurl": service_url,
    })
}

/// What the Bot Framework sends the bot `app` of the test directory when
/// the person `aad`, named `name`, writes `text` in their personal
/// conversation with it, as its activity `id`, from the service at
/// `service_url`.
pub(super) fn message(
    service_url: &str,
    app: &str,
    id: &str,
    (aad, name): (&str, &str),
    text: &str,
) -> Value {
    json!({
        "type": "message",
        "id": id,
        "timestamp": "2026-10-30T09:00:00.000Z",
        "serviceUrl": service_url,
        "channelId": "msteams",
        "from": { "id": member_id(aad), "name": name, "aadObjectId": aad },
        "conversation": {
            "conversationType": "personal",
            "tenantId": TEAMS_DIRECTORY,
            "id": conversation_of(aad),
        },
        "recipient": { "id": format!("28:{app}"), "name": "Assistant" },
        "text": text,
        "textFormat": "plain",
        "locale": "fr-FR",
        "channelData": { "tenant": { "id": TEAMS_DIRECTORY } },
    })
}

/// `GET /test/messages`: the messages the bots wrote, oldest first.
pub(super) async fn messages(State(shared): SharedState) -> Json<Value> {
    let shared = shared.lock().expect("lock");
    let messages: Vec<Value> = (shared.sent.iter())
        .map(|m| json!({ "app": m.app, "conversation": m.conversation, "text": m.text }))
        .collect();
    Json(json!(messages))
}

/// Body of `POST /test/activity`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Written {
    app: String,
    id: String,
    aad: String,
    name: String,
    text: String,
}

/// `POST /test/activity`: the activity the person `aad` writing `text` to
/// the bot `app` would be, and the `Authorization` header it comes with.
pub(super) async fn activity(
    State(shared): SharedState,
    Json(written): Json<Written>,
) -> Json<Value> {
    let service_url = format!("{}/teams/", shared.lock().expect("lock").base);
    let person = (written.aad.as_str(), written.name.as_str());
    let token = signed_token(Signer::BotFramework, &claims(&service_url, &written.app));
    Json(json!({
        "authorization": format!("Bearer {token}"),
        "activity": message(&service_url, &written.app, &written.id, person, &written.text),
    }))
}
