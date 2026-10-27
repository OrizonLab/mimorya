//! What the test Graph writes (ADR-119): an e-mail accepted for sending
//! (202, no body) and an event created (201, its identifier naming its
//! owner), each recorded with what it received; or refused with the
//! status a test chose.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::graph::{Shared, failure, owner};

/// The owner of the token, once the write is recorded and not refused.
fn writer(
    shared: &Shared,
    headers: &HeaderMap,
    path: &str,
    body: Value,
) -> Result<String, StatusCode> {
    let owner = owner(shared, headers, path.into(), &HashMap::new(), Some(body))?;
    match shared.lock().expect("graph").refused_writes {
        Some(status) => Err(status),
        None => Ok(owner),
    }
}

pub(super) async fn send_mail(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    match writer(&shared, &headers, "/me/sendMail", body) {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(refused) => failure(refused),
    }
}

pub(super) async fn create_event(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let owner = match writer(&shared, &headers, "/me/events", body.clone()) {
        Ok(owner) => owner,
        Err(refused) => return failure(refused),
    };
    let created = json!({
        "id": format!("{owner}-nouveau"),
        "subject": body["subject"],
        "start": body["start"],
        "end": body["end"],
        "webLink": format!("https://outlook.example/{owner}/nouveau"),
    });
    (StatusCode::CREATED, Json(created)).into_response()
}
