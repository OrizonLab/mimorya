//! An OpenID Connect provider for the tests, on a local port (reachable when
//! the operator allows the private network): discovery, keys and token
//! endpoint over HTTP, as the server reaches them; the browser's visit to
//! the authorization address is played in-process (`authorize`), signed in
//! as the person the test chose. Its signing key is a test key, public.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use openidconnect::core::{
    CoreGenderClaim, CoreJsonWebKeySet, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
    CoreRsaPrivateSigningKey,
};
use openidconnect::{
    AdditionalClaims, Audience, EndUserEmail, EndUserUsername, IdToken, IdTokenClaims, IssuerUrl,
    JsonWebKeyId, Nonce, PrivateSigningKey, StandardClaims, SubjectIdentifier,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use url::Url;

pub const CLIENT_ID: &str = "mimorya-tests";
pub const CLIENT_SECRET: &str = "secret-client-de-test-0123456789";
const TEST_KEY: &str = include_str!("identity_provider_key.pem");

/// Who signs in at the provider, as its token says.
#[derive(Debug, Clone, Default)]
pub struct Person {
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    /// Microsoft: the sign-in name in the directory.
    pub preferred_username: Option<String>,
    /// Other claims (`oid`, `hd`…).
    pub extra: Map<String, Value>,
}

impl Person {
    /// An address the provider vouches for.
    pub fn verified(subject: &str, email: &str) -> Self {
        Self {
            subject: subject.into(),
            email: Some(email.into()),
            email_verified: Some(true),
            ..Self::default()
        }
    }
}

/// A token the provider should not have issued, for refusals.
#[derive(Debug, Clone, Default)]
pub struct Tamper {
    pub audience: Option<String>,
    pub nonce: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Extra {
    #[serde(flatten)]
    claims: Map<String, Value>,
}

impl AdditionalClaims for Extra {}

struct Grant {
    person: Person,
    nonce: Option<String>,
    challenge: String,
    redirect_uri: String,
}

#[derive(Default)]
struct Provider {
    issuer: String,
    signed_in: Option<Person>,
    tamper: Tamper,
    grants: HashMap<String, Grant>,
    /// How many codes were exchanged.
    exchanges: usize,
}

type Shared = Arc<Mutex<Provider>>;

pub struct MockIdentityProvider {
    pub issuer: String,
    shared: Shared,
}

impl MockIdentityProvider {
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("local port");
        let issuer = format!("http://{}", listener.local_addr().expect("address"));
        let shared: Shared = Arc::new(Mutex::new(Provider {
            issuer: issuer.clone(),
            ..Provider::default()
        }));
        let router = Router::new()
            .route("/.well-known/openid-configuration", get(configuration))
            .route("/jwks", get(keys))
            .route("/token", post(token))
            .with_state(shared.clone());
        tokio::spawn(async move { axum::serve(listener, router).await });
        Self { issuer, shared }
    }

    /// The person the next authorization signs in.
    pub fn sign_in_as(&self, person: Person) {
        self.shared.lock().expect("provider").signed_in = Some(person);
    }

    pub fn tamper(&self, tamper: Tamper) {
        self.shared.lock().expect("provider").tamper = tamper;
    }

    pub fn exchanges(&self) -> usize {
        self.shared.lock().expect("provider").exchanges
    }

    /// The browser's visit to the authorization address: the query of the
    /// return to the application (`code`, `state`), and its path.
    pub fn authorize(&self, address: &str) -> String {
        let url = Url::parse(address).expect("authorization address");
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert!(
            address.starts_with(&format!("{}/authorize?", self.issuer)),
            "{address}"
        );
        assert_eq!(params["response_type"], "code");
        assert_eq!(params["client_id"], CLIENT_ID);
        assert_eq!(params["code_challenge_method"], "S256");
        let scopes: Vec<&str> = params["scope"].split(' ').collect();
        assert!(
            scopes.contains(&"openid") && scopes.contains(&"email"),
            "{scopes:?}"
        );
        let mut provider = self.shared.lock().expect("provider");
        let person = provider.signed_in.clone().expect("someone signs in");
        let code = URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes());
        provider.grants.insert(
            code.clone(),
            Grant {
                person,
                nonce: params.get("nonce").cloned(),
                challenge: params["code_challenge"].clone(),
                redirect_uri: params["redirect_uri"].clone(),
            },
        );
        let back: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("code", &code)
            .append_pair("state", &params["state"])
            .finish();
        format!("/auth/sso/callback?{back}")
    }
}

fn signing_key() -> CoreRsaPrivateSigningKey {
    CoreRsaPrivateSigningKey::from_pem(TEST_KEY, Some(JsonWebKeyId::new("test".into())))
        .expect("test key")
}

async fn configuration(State(shared): State<Shared>) -> Response {
    let issuer = shared.lock().expect("provider").issuer.clone();
    let body = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/jwks"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "token_endpoint_auth_methods_supported": ["client_secret_post", "client_secret_basic"],
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

async fn keys() -> Response {
    let set = CoreJsonWebKeySet::new(vec![signing_key().as_verification_key()]);
    let body = serde_json::to_string(&set).expect("key set");
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

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

async fn token(State(shared): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
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
        &signing_key(),
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
