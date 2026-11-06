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
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::hmac;
use serde_json::{Value, json};

use super::authorization_server::{CLIENT_SECRET, FakeAuthorizationServer, Settings};
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
    /// Another attempt's nonce.
    OtherNonce,
    /// Issued to another client.
    OtherAudience,
    /// Issued by another issuer than the server.
    OtherIssuer,
    /// Signed with a key the server never published.
    Forged,
    /// Signed with the test client's secret (HS256), which a client
    /// without a secret never takes.
    SignedWithSecret,
    /// Expired an hour ago.
    Expired,
}

/// Its OpenID configuration, naming its key set: absent until someone is
/// signed in; kept back at will (`held_configuration`), each request
/// counted.
pub(super) async fn configuration(State(server): State<FakeAuthorizationServer>) -> Response {
    let settings = {
        let mut inner = server.inner.lock().expect("server");
        inner.configuration_asked += 1;
        inner.settings.clone()
    };
    if let Some(held) = settings.held_configuration {
        tokio::time::sleep(held).await;
    }
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
        "id_token_signing_alg_values_supported": ["RS256", "HS256"],
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
    let issued_by = match quirk {
        IdTokenQuirk::OtherIssuer => format!("{issuer}/autre"),
        _ => issuer.to_string(),
    };
    let mut claims = json!({
        "iss": issued_by,
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
        IdTokenQuirk::SignedWithSecret => return Some(signed_with_secret(&claims)),
        IdTokenQuirk::Forged => Signer::Forger,
        _ => Signer::BotFramework,
    };
    Some(signed_as(signer, KEY_ID, &claims))
}

/// `claims` signed with the test client's secret (HS256).
fn signed_with_secret(claims: &Value) -> String {
    let header = json!({ "alg": "HS256", "typ": "JWT" });
    let input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let key = hmac::Key::new(hmac::HMAC_SHA256, CLIENT_SECRET.as_bytes());
    let tag = hmac::sign(&key, input.as_bytes());
    format!("{input}.{}", URL_SAFE_NO_PAD.encode(tag.as_ref()))
}
