//! OpenID Connect at the simulated authorization server (ADR-175), once a
//! test signs someone in there (`Settings::identity`): its configuration,
//! its key set (the test key the test Bot Framework signs with, under
//! another name), and, for a code asked with `openid`, an identity token
//! naming that person, signed with that key, for the client the code was
//! given to and the nonce of its attempt; or one a test spoiled
//! (`IdTokenQuirk`). Without someone signed in, it is no OpenID provider.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::authorization_server::{FakeAuthorizationServer, Settings};
use crate::testing::teams::{Signer, published_key, signed_as};

/// The name the server publishes its key under.
const KEY_ID: &str = "cle-test-serveur-d-autorisation";

/// The person signed in at the server, as its identity tokens name them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedIn {
    pub subject: String,
    pub email: String,
    pub email_verified: bool,
}

impl SignedIn {
    /// Someone whose address the server verified.
    pub fn verified(email: &str) -> Self {
        Self {
            subject: format!("sujet-{email}"),
            email: email.into(),
            email_verified: true,
        }
    }
}

/// How the server spoils the identity token of a code asked with `openid`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IdTokenQuirk {
    #[default]
    None,
    /// No identity token at all.
    Missing,
    /// Another attempt's nonce, or none.
    OtherNonce,
    /// Issued to another client.
    OtherAudience,
    /// Signed with a key the server never published.
    Forged,
    /// Expired an hour ago.
    Expired,
}

/// Its OpenID configuration, naming its key set: absent until someone is
/// signed in.
pub(super) async fn configuration(State(server): State<FakeAuthorizationServer>) -> Response {
    let settings = server.inner.lock().expect("server").settings.clone();
    if settings.identity.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let issuer = &server.issuer;
    axum::Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/jwks"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "scopes_supported": ["openid", "email", "profile", "files:read", "offline_access"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": settings.auth_methods,
    }))
    .into_response()
}

/// The key set its identity tokens are signed by.
pub(super) async fn keys() -> Response {
    axum::Json(json!({ "keys": [published_key(KEY_ID)] })).into_response()
}

/// The identity token of a code `client_id` asked with `scope`, its
/// attempt's `nonce`, if it asked `openid` and someone is signed in.
pub(super) fn id_token(
    issuer: &str,
    settings: &Settings,
    client_id: &str,
    scope: &str,
    nonce: Option<&str>,
) -> Option<String> {
    let person = settings.identity.as_ref()?;
    let quirk = settings.id_token_quirk;
    if !scope.split(' ').any(|scope| scope == "openid") || quirk == IdTokenQuirk::Missing {
        return None;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (issued, expires) = match quirk {
        IdTokenQuirk::Expired => (now - 7200, now - 3600),
        _ => (now, now + 600),
    };
    let audience = match quirk {
        IdTokenQuirk::OtherAudience => "un-autre-client",
        _ => client_id,
    };
    let mut claims = json!({
        "iss": issuer,
        "sub": person.subject,
        "aud": audience,
        "iat": issued,
        "exp": expires,
        "email": person.email,
        "email_verified": person.email_verified,
    });
    match (quirk, nonce) {
        (IdTokenQuirk::OtherNonce, _) => {
            claims["nonce"] = Value::from("nonce-d-une-autre-tentative")
        }
        (_, Some(nonce)) => claims["nonce"] = Value::from(nonce),
        (_, None) => {}
    }
    let signer = match quirk {
        IdTokenQuirk::Forged => Signer::Forger,
        _ => Signer::BotFramework,
    };
    Some(signed_as(signer, KEY_ID, &claims))
}
