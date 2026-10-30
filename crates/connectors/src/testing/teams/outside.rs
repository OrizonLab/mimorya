//! What the end-to-end tests ask the test Microsoft from outside
//! (ADR-139): the messages the bots wrote, and the activity a person's
//! message to a bot would be (`activity`), with its token (`signing`), for
//! them to send the bot's messaging endpoint.

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

use super::Shared;
use super::activity::{claims, message, service_url};
use super::signing::{Signer, signed_token};

type SharedState = State<Arc<Mutex<Shared>>>;

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
    let service_url = service_url(&shared.lock().expect("lock").base);
    let person = (written.aad.as_str(), written.name.as_str());
    let token = signed_token(Signer::BotFramework, &claims(&service_url, &written.app));
    Json(json!({
        "authorization": format!("Bearer {token}"),
        "activity": message(&service_url, &written.app, &written.id, person, &written.text),
    }))
}
