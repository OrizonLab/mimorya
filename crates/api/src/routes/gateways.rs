//! Where Slack sends a company's gateway its events (ADR-125), before any
//! session: the request's signature, made with the gateway's signing
//! secret, is its only proof, so the request header the browser's requests
//! carry is not asked of it (`plugins/csrf.rs`). An unknown gateway and a
//! request not signed by its application answer alike (404). At most
//! `MAX_BODY_BYTES` are read (413). Slack's address check is answered with
//! its challenge; anything else signed is answered 200, naming nothing,
//! whatever became of it: a linked person's message is answered in the
//! background (ADR-127), Slack expecting its answer within seconds, and so
//! is a validation button pressed, which Slack sends form-encoded to the
//! same address (ADR-129).

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use mimorya_core::DomainError;
use serde_json::json;
use uuid::Uuid;

use crate::app::AppState;
use crate::http::{ApiError, ApiResult};
use crate::services::gateways::{EventOutcome, SlackHeaders, converse, decide_in_slack};

pub use crate::services::gateways::SLACK_EVENTS_PREFIX;

/// What Slack may send: its events are a few kibibytes.
const MAX_BODY_BYTES: usize = 256 * 1024;
const UNKNOWN: &str = "Passerelle introuvable";
const TOO_LARGE: &str = "Événement trop volumineux";
const UNREADABLE: &str = "Événement illisible";

pub fn routes() -> Router<AppState> {
    Router::new().route("/gateways/slack/{id}", post(receive))
}

async fn receive(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<Response> {
    let Ok(id) = Uuid::parse_str(&raw) else {
        return Err(DomainError::not_found(UNKNOWN).into());
    };
    let announced = headers
        .get(CONTENT_LENGTH)
        .and_then(|length| length.to_str().ok())
        .and_then(|length| length.parse::<u64>().ok());
    if announced.is_some_and(|length| length > MAX_BODY_BYTES as u64) {
        return Err(ApiError::request(StatusCode::PAYLOAD_TOO_LARGE, TOO_LARGE));
    }
    let bytes = axum::body::to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| ApiError::request(StatusCode::PAYLOAD_TOO_LARGE, TOO_LARGE))?;
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let signed = SlackHeaders {
        timestamp: header("x-slack-request-timestamp"),
        signature: header("x-slack-signature"),
        content_type: header(CONTENT_TYPE.as_str()),
    };
    match state
        .gateways
        .receive_slack(id, signed, &bytes, Utc::now())
        .await?
    {
        EventOutcome::Challenge(challenge) => {
            Ok(Json(json!({ "challenge": challenge })).into_response())
        }
        EventOutcome::Taken => Ok(StatusCode::OK.into_response()),
        EventOutcome::Message(incoming) => {
            let services = state.clone();
            (state.background).spawn(async move { converse(&services, *incoming).await });
            Ok(StatusCode::OK.into_response())
        }
        EventOutcome::Choice(pressed) => {
            let services = state.clone();
            (state.background).spawn(async move { decide_in_slack(&services, *pressed).await });
            Ok(StatusCode::OK.into_response())
        }
        EventOutcome::Unknown => Err(DomainError::not_found(UNKNOWN).into()),
        EventOutcome::Unreadable => Err(ApiError::request(StatusCode::BAD_REQUEST, UNREADABLE)),
    }
}
