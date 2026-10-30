//! What the end-to-end tests ask the test Microsoft from outside
//! (ADR-139): the messages the bots wrote (with their card, ADR-141), and
//! the activity a person's message to a bot, or a button they press on one
//! of its cards, would be (`activity`), with its token (`signing`), for
//! them to send the bot's messaging endpoint.

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

use super::Shared;
use super::activity::{claims, message, pressed, service_url};
use super::signing::{Signer, signed_token};

type SharedState = State<Arc<Mutex<Shared>>>;

/// `GET /test/messages`: the messages the bots wrote, oldest first.
pub(super) async fn messages(State(shared): SharedState) -> Json<Value> {
    let shared = shared.lock().expect("lock");
    let messages: Vec<Value> = (shared.sent.iter())
        .map(|m| {
            json!({
                "id": m.id,
                "app": m.app,
                "conversation": m.conversation,
                "text": m.text,
                "textFormat": m.text_format,
                "card": m.card,
            })
        })
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
    #[serde(default)]
    text: String,
    /// A button pressed instead: what it carries, on which card.
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    card: Option<String>,
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
        "activity": match (written.value, written.card) {
            (Some(value), Some(card)) => {
                pressed(&service_url, (&written.app, &written.id), person, (&card, value))
            }
            _ => message(&service_url, &written.app, &written.id, person, &written.text),
        },
    }))
}
