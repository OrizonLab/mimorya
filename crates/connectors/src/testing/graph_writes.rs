//! What the test Graph writes (ADR-119): an e-mail accepted for sending
//! (202, no body) and an event created (201, its identifier naming its
//! owner, its times in the platform's zone when asked so), each recorded
//! with what it received; or refused with the status a test chose.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{NaiveDateTime, TimeZone, Utc};
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use serde_json::{Value, json};

use super::graph::{Shared, failure, owner};

/// A time sent in UTC, as Graph gives it back in the platform's zone when
/// asked (`Prefer: outlook.timezone`).
fn as_asked(time: &Value, headers: &HeaderMap) -> Value {
    let asked = headers
        .get("prefer")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|prefer| prefer.contains(PLATFORM_TIME_ZONE.name()));
    let utc = time["dateTime"]
        .as_str()
        .and_then(|at| NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M:%S").ok())
        .filter(|_| asked && time["timeZone"] == "UTC");
    match utc {
        Some(at) => {
            let local = Utc
                .from_utc_datetime(&at)
                .with_timezone(&PLATFORM_TIME_ZONE);
            json!({
                "dateTime": local.format("%Y-%m-%dT%H:%M:%S.0000000").to_string(),
                "timeZone": PLATFORM_TIME_ZONE.name(),
            })
        }
        None => time.clone(),
    }
}

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
        "start": as_asked(&body["start"], &headers),
        "end": as_asked(&body["end"], &headers),
        "webLink": format!("https://outlook.example/{owner}/nouveau"),
    });
    (StatusCode::CREATED, Json(created)).into_response()
}
