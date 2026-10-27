//! The provider's renewals (ADR-117): a refresh token it issued, unless
//! revoked, brings an access token `acces-<subject>-<n>`, as the test
//! Graph reads them, valid an hour; and a new refresh token, when it
//! rotates them (the old one stays valid, as at Microsoft).

use std::collections::HashMap;

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Default)]
pub(super) struct Renewals {
    /// The refresh tokens issued, and whose.
    pub issued: HashMap<String, String>,
    /// The permissions asked at each renewal.
    pub asked: Vec<String>,
    pub revoked: bool,
    pub rotate: bool,
    /// Access tokens minted.
    minted: usize,
}

pub(super) fn renew(renewals: &mut Renewals, form: &HashMap<String, String>) -> Response {
    let subject = form
        .get("refresh_token")
        .and_then(|token| renewals.issued.get(token))
        .cloned();
    let Some(subject) = subject.filter(|_| !renewals.revoked) else {
        let body = json!({ "error": "invalid_grant", "error_description": "AADSTS70000" });
        return (StatusCode::BAD_REQUEST, axum::Json(body)).into_response();
    };
    renewals
        .asked
        .push(form.get("scope").cloned().unwrap_or_default());
    renewals.minted += 1;
    let mut body = json!({
        "access_token": format!("acces-{subject}-{}", renewals.minted),
        "token_type": "Bearer",
        "expires_in": 3600,
    });
    if renewals.rotate {
        let rotated = format!("jeton-de-renouvellement-{subject}-{}", renewals.minted);
        renewals.issued.insert(rotated.clone(), subject);
        body["refresh_token"] = json!(rotated);
    }
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}
