//! The provider's revocation endpoint (ADR-194), as Google's: a refresh
//! token sent alone (`token`) is revoked, its renewals refused from then
//! on; one it never issued answers `invalid_token`. A test may have it
//! refuse every revocation, keep them waiting beyond any bound, or not
//! name its endpoint in its discovery.

use axum::Form;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::collections::HashMap;

use super::Shared;

#[derive(Default)]
pub struct Revocations {
    /// The tokens revoked, in order.
    pub revoked: Vec<String>,
    /// Every revocation answered by a refusal.
    pub refused: bool,
    /// Every revocation kept waiting, never answered in time.
    pub held: bool,
    /// How many revocations were asked, answered or not.
    pub asked: usize,
    /// Its discovery names no revocation endpoint.
    pub unnamed: bool,
}

pub(super) async fn revoke(
    State(shared): State<Shared>,
    Form(params): Form<HashMap<String, String>>,
) -> Response {
    let held = {
        let mut provider = shared.lock().expect("provider");
        provider.requests += 1;
        provider.revocations.asked += 1;
        provider.revocations.held
    };
    if held {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
    let mut provider = shared.lock().expect("provider");
    if provider.revocations.refused {
        return error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable");
    }
    // Google's: the token alone.
    if params.len() != 1 {
        return error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    let Some(token) = params.get("token") else {
        return error(StatusCode::BAD_REQUEST, "invalid_request");
    };
    if !token.starts_with("jeton-de-renouvellement-") {
        return error(StatusCode::BAD_REQUEST, "invalid_token");
    }
    provider.renewals.issued.remove(token);
    provider.revocations.revoked.push(token.clone());
    StatusCode::OK.into_response()
}

fn error(status: StatusCode, code: &str) -> Response {
    let body = json!({ "error": code, "error_description": "jeton-de-renouvellement-secret" });
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}
