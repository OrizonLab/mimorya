//! The provider's token endpoint: the client authenticated (in the body or
//! with Basic), a code redeemed once against its PKCE challenge, and an
//! identity token signed for the person it granted.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use openidconnect::core::{
    CoreGenderClaim, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
};
use openidconnect::{
    Audience, EndUserEmail, EndUserUsername, IdToken, IdTokenClaims, IssuerUrl, Nonce,
    StandardClaims, SubjectIdentifier,
};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{CLIENT_ID, CLIENT_SECRET, Extra, Shared, signing_key};

fn refused(error: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        axum::Json(json!({ "error": error })),
    )
        .into_response()
}

/// The client authenticates in the body or with Basic.
fn authenticated(headers: &HeaderMap, form: &HashMap<String, String>) -> bool {
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| STANDARD.decode(v).ok())
        .and_then(|v| String::from_utf8(v).ok());
    let (id, secret) = match basic {
        Some(pair) => match pair.split_once(':') {
            Some((id, secret)) => (id.to_string(), secret.to_string()),
            None => return false,
        },
        None => match (form.get("client_id"), form.get("client_secret")) {
            (Some(id), Some(secret)) => (id.clone(), secret.clone()),
            _ => return false,
        },
    };
    id == CLIENT_ID && secret == CLIENT_SECRET
}

pub(super) async fn token(
    State(shared): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    shared.lock().expect("provider").requests += 1;
    let form: HashMap<String, String> = url::form_urlencoded::parse(&body).into_owned().collect();
    if !authenticated(&headers, &form) {
        return refused("invalid_client");
    }
    let mut provider = shared.lock().expect("provider");
    // A code is redeemed once.
    let Some(grant) = form.get("code").and_then(|c| provider.grants.remove(c)) else {
        return refused("invalid_grant");
    };
    let verifier = form.get("code_verifier").cloned().unwrap_or_default();
    if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != grant.challenge
        || form.get("redirect_uri") != Some(&grant.redirect_uri)
        || form.get("grant_type").map(String::as_str) != Some("authorization_code")
    {
        return refused("invalid_grant");
    }
    provider.exchanges += 1;
    let now = Utc::now();
    let person = grant.person;
    let mut standard = StandardClaims::new(SubjectIdentifier::new(person.subject));
    if let Some(email) = person.email {
        standard = standard.set_email(Some(EndUserEmail::new(email)));
    }
    standard = standard
        .set_email_verified(person.email_verified)
        .set_preferred_username(person.preferred_username.map(EndUserUsername::new));
    let audience = provider.tamper.audience.clone().unwrap_or(CLIENT_ID.into());
    let claims = IdTokenClaims::<Extra, CoreGenderClaim>::new(
        IssuerUrl::new(provider.issuer.clone()).expect("issuer"),
        vec![Audience::new(audience)],
        now + Duration::minutes(5),
        now,
        standard,
        Extra {
            claims: person.extra,
        },
    )
    .set_nonce(
        provider
            .tamper
            .nonce
            .clone()
            .or(grant.nonce)
            .map(Nonce::new),
    );
    let id_token = IdToken::<
        Extra,
        CoreGenderClaim,
        CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm,
    >::new(
        claims,
        &signing_key(
            provider.rolled_over != provider.tamper.unpublished_key,
            !provider.without_key_ids,
        ),
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
        None,
        None,
    )
    .expect("signed token");
    let body = json!({
        "access_token": "jeton-d-acces-de-test",
        "token_type": "Bearer",
        "expires_in": 300,
        "id_token": id_token.to_string(),
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}
