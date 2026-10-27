//! An OpenID Connect provider for the tests, on a local port (reachable when
//! the operator allows the private network): discovery, keys and token
//! endpoint over HTTP, as the server reaches them. The browser's visit to
//! the authorization address is played in-process by the API's tests
//! (`authorize`), or served (`/authorize`) to the end-to-end tests' browser;
//! either signs in the person the test chose, else the one the address
//! hints at; the browser, hinted at nobody, is asked who (`sign_in_page`).
//! Its signing keys are test keys, public: it rolls them over on demand,
//! and may be unavailable.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use openidconnect::core::{CoreJsonWebKeySet, CoreRsaPrivateSigningKey};
use openidconnect::{AdditionalClaims, JsonWebKeyId, PrivateSigningKey};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use url::Url;

pub const CLIENT_ID: &str = "mimorya-tests";
pub const CLIENT_SECRET: &str = "secret-client-de-test-0123456789";

#[path = "identity_provider/sign_in_page.rs"]
mod sign_in_page;
#[path = "identity_provider/token.rs"]
mod token;

const TEST_KEY: &str = include_str!("identity_provider_key.pem");
/// The key it signs with once rolled over.
const NEXT_KEY: &str = include_str!("identity_provider_key_2.pem");

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
    /// Signed by a key it does not publish.
    pub unpublished_key: bool,
    /// No refresh token, though offline access was asked (ADR-116).
    pub withhold_refresh_token: bool,
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
    /// The permissions asked: offline access brings a refresh token.
    scope: String,
}

#[derive(Default)]
struct Provider {
    issuer: String,
    signed_in: Option<Person>,
    tamper: Tamper,
    grants: HashMap<String, Grant>,
    /// How many codes were exchanged.
    exchanges: usize,
    /// How many requests reached it (discovery, keys, tokens).
    requests: usize,
    /// How many times its declaration, and its keys, were read.
    discoveries: usize,
    key_reads: usize,
    /// Signing with its next key, the only one published.
    rolled_over: bool,
    /// Its keys published, and its tokens signed, without an identifier.
    without_key_ids: bool,
    /// Keys published beside its own, signing nothing.
    extra_keys: usize,
    /// Its declaration answered by an error.
    unavailable: bool,
}

type Shared = Arc<Mutex<Provider>>;

pub struct MockIdentityProvider {
    pub issuer: String,
    shared: Shared,
}

impl MockIdentityProvider {
    pub async fn start() -> Self {
        Self::start_on("127.0.0.1:0").await
    }

    /// On a given address (the end-to-end tests declare it in advance).
    pub async fn start_on(address: &str) -> Self {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .unwrap_or_else(|error| {
                panic!("identity provider: cannot listen on {address}: {error}")
            });
        let issuer = format!("http://{}", listener.local_addr().expect("address"));
        let shared: Shared = Arc::new(Mutex::new(Provider {
            issuer: issuer.clone(),
            ..Provider::default()
        }));
        let router = Router::new()
            .route("/.well-known/openid-configuration", get(configuration))
            .route("/jwks", get(keys))
            .route("/authorize", get(visit))
            .route("/token", post(token::token))
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

    pub fn requests(&self) -> usize {
        self.shared.lock().expect("provider").requests
    }

    /// How many times its declaration, then its keys, were read.
    pub fn reads(&self) -> (usize, usize) {
        let provider = self.shared.lock().expect("provider");
        (provider.discoveries, provider.key_reads)
    }

    /// Signs with its next key from now on, and publishes it alone.
    pub fn roll_keys_over(&self) {
        self.shared.lock().expect("provider").rolled_over = true;
    }

    pub fn set_unavailable(&self, unavailable: bool) {
        self.shared.lock().expect("provider").unavailable = unavailable;
    }

    /// Publishes its keys, and signs, without key identifiers (OpenID
    /// Connect allows it for a single key).
    pub fn without_key_ids(&self) {
        self.shared.lock().expect("provider").without_key_ids = true;
    }

    /// Publishes this many keys more, as a heavy key set.
    pub fn publish_extra_keys(&self, count: usize) {
        self.shared.lock().expect("provider").extra_keys = count;
    }

    /// The browser's visit to the authorization address: the return to the
    /// application, its path under `/api` and its query (`code`, `state`).
    pub fn authorize(&self, address: &str) -> String {
        let url = Url::parse(address).expect("authorization address");
        assert!(
            address.starts_with(&format!("{}/authorize?", self.issuer)),
            "{address}"
        );
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let scopes: Vec<&str> = params["scope"].split(' ').collect();
        assert!(
            scopes.contains(&"openid") && scopes.contains(&"email"),
            "{scopes:?}"
        );
        let (back, redirect_uri) = self
            .shared
            .lock()
            .expect("provider")
            .grant(&params)
            .expect("authorization granted");
        let redirect = Url::parse(&redirect_uri).expect("redirect address");
        let path = redirect.path();
        format!("{}?{back}", path.strip_prefix("/api").unwrap_or(path))
    }

    /// The refresh token it issued for this person's subject.
    pub fn refresh_token_of(subject: &str) -> String {
        format!("jeton-de-renouvellement-{subject}")
    }
}

impl Provider {
    /// A code for this authorization request, signed in as the person
    /// chosen, else as the one hinted at: the return's query and address.
    fn grant(&mut self, params: &HashMap<String, String>) -> Result<(String, String), String> {
        let param = |name: &str| {
            params
                .get(name)
                .cloned()
                .ok_or_else(|| format!("missing {name}"))
        };
        if param("response_type")? != "code"
            || param("client_id")? != CLIENT_ID
            || param("code_challenge_method")? != "S256"
        {
            return Err("unexpected request".into());
        }
        let person = match (&self.signed_in, params.get("login_hint")) {
            (Some(person), _) => person.clone(),
            (None, Some(email)) => {
                let mut person = Person::verified(&format!("sujet-{email}"), email);
                // As the provider's own sign-in page names them.
                if let Some(name) = params.get("name").filter(|n| !n.is_empty()) {
                    person
                        .extra
                        .insert("name".into(), Value::String(name.clone()));
                }
                person
            }
            (None, None) => return Err("nobody signs in".into()),
        };
        let code = URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes());
        let redirect_uri = param("redirect_uri")?;
        self.grants.insert(
            code.clone(),
            Grant {
                person,
                nonce: params.get("nonce").cloned(),
                challenge: param("code_challenge")?,
                redirect_uri: redirect_uri.clone(),
                scope: param("scope")?,
            },
        );
        let back = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("code", &code)
            .append_pair("state", &param("state")?)
            .finish();
        Ok((back, redirect_uri))
    }
}

/// The browser's visit, served: straight back to the application.
async fn visit(
    State(shared): State<Shared>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let mut provider = shared.lock().expect("provider");
    if provider.signed_in.is_none() && !params.contains_key("login_hint") {
        return sign_in_page::sign_in_page(&params);
    }
    match provider.grant(&params) {
        Ok((back, redirect_uri)) => (
            StatusCode::FOUND,
            [(header::LOCATION, format!("{redirect_uri}?{back}"))],
        )
            .into_response(),
        Err(problem) => (StatusCode::BAD_REQUEST, problem).into_response(),
    }
}

fn signing_key(next: bool, with_id: bool) -> CoreRsaPrivateSigningKey {
    let (pem, id) = if next {
        (NEXT_KEY, "test-2")
    } else {
        (TEST_KEY, "test")
    };
    let id = with_id.then(|| JsonWebKeyId::new(id.into()));
    CoreRsaPrivateSigningKey::from_pem(pem, id).expect("test key")
}

async fn configuration(State(shared): State<Shared>) -> Response {
    let issuer = {
        let mut provider = shared.lock().expect("provider");
        provider.requests += 1;
        provider.discoveries += 1;
        if provider.unavailable {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        provider.issuer.clone()
    };
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

async fn keys(State(shared): State<Shared>) -> Response {
    let (next, with_id, extra) = {
        let mut provider = shared.lock().expect("provider");
        provider.requests += 1;
        provider.key_reads += 1;
        (
            provider.rolled_over,
            !provider.without_key_ids,
            provider.extra_keys,
        )
    };
    let mut keys = vec![signing_key(next, with_id).as_verification_key()];
    for n in 0..extra {
        let id = Some(JsonWebKeyId::new(format!("extra-{n}")));
        let key = CoreRsaPrivateSigningKey::from_pem(NEXT_KEY, id).expect("test key");
        keys.push(key.as_verification_key());
    }
    let set = CoreJsonWebKeySet::new(keys);
    let body = serde_json::to_string(&set).expect("key set");
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}
