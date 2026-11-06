//! An OAuth authorization server, as an MCP server's would be: its metadata
//! (RFC 8414), an authorization endpoint answering at once (no one to sign
//! in, the code sent back to the redirect address), its token endpoint
//! (`token_endpoint`), and those that revoke tokens and register clients;
//! an OpenID provider too once a test signs someone in there (`openid`,
//! ADR-175). Every request is recorded; the tests turn its behaviour
//! (`Settings`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};

use super::openid::{IdTokenQuirk, SignedIn};
use super::{
    metadata_endpoint, openid, registration_endpoint, revocation_endpoint, token_endpoint,
};
use crate::outbound::{NetworkPolicy, http_client};
use crate::testing::Hold;

pub const CLIENT_ID: &str = "mimorya-test-client";
pub const CLIENT_SECRET: &str = "secret-du-client-de-test";

/// How the server behaves; changed by the tests.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Seconds an access token lasts; `None`: no `expires_in`.
    pub lifetime: Option<u64>,
    /// Whether a refresh token comes with the access token.
    pub refresh: bool,
    /// Tokens with characters RFC 6750 does not allow.
    pub off_alphabet: bool,
    /// Access tokens of `LONG_TOKEN_CHARS`, as a JWT full of claims can be.
    pub long_tokens: bool,
    /// Whether the return names the issuer (RFC 9207), as its metadata says.
    pub iss_on_return: bool,
    /// Metadata naming another issuer than the server's.
    pub wrong_issuer: bool,
    /// What `token_endpoint_auth_methods_supported` lists.
    pub auth_methods: Vec<String>,
    /// Whether its metadata names its revocation endpoint (RFC 7009).
    pub revocation: bool,
    /// A revocation endpoint that takes the request and does not answer.
    pub mute_revocation: bool,
    /// Whether it revokes access tokens; if not, it answers
    /// `unsupported_token_type` for one (RFC 7009 §2.2.1).
    pub revokes_access: bool,
    /// A renewal's answer kept back this long, its tokens issued.
    pub held_renewals: Option<Duration>,
    /// Whether its metadata names its registration endpoint (RFC 7591).
    pub registration: bool,
    /// A registration endpoint that asks for an initial access token.
    pub registration_token: bool,
    /// Seconds a registered client's secret lasts; `None`: it does not
    /// expire.
    pub secret_lifetime: Option<u64>,
    /// Who is signed in at the server, as an OpenID provider: `None`, it
    /// is none, and gives no identity token.
    pub identity: Option<SignedIn>,
    /// How it spoils the identity tokens it gives.
    pub id_token_quirk: IdTokenQuirk,
    /// Its metadata held until the test lets it through: a discovery
    /// that takes its time.
    pub metadata_hold: Option<Arc<Hold>>,
    /// Its OpenID configuration held the same way: a verification of an
    /// identity token that takes its time.
    pub configuration_hold: Option<Arc<Hold>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lifetime: Some(3600),
            refresh: true,
            off_alphabet: false,
            long_tokens: false,
            iss_on_return: false,
            wrong_issuer: false,
            auth_methods: ["client_secret_basic", "client_secret_post", "none"]
                .map(String::from)
                .to_vec(),
            revocation: true,
            mute_revocation: false,
            revokes_access: true,
            held_renewals: None,
            registration: true,
            registration_token: false,
            secret_lifetime: None,
            identity: None,
            id_token_quirk: IdTokenQuirk::None,
            metadata_hold: None,
            configuration_hold: None,
        }
    }
}

/// A request the server received: its endpoint and its parameters.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub endpoint: &'static str,
    pub params: HashMap<String, String>,
    pub basic: Option<String>,
}

pub(super) struct Code {
    pub(super) redirect_uri: String,
    pub(super) challenge: String,
    pub(super) resource: Option<String>,
    pub(super) scope: String,
    /// The client it was given to, and the nonce its attempt sent.
    pub(super) client_id: String,
    pub(super) nonce: Option<String>,
}

/// A client registered at the server (RFC 7591).
pub(super) struct Client {
    pub(super) secret: Option<String>,
    /// How it proves itself, as registered.
    pub(super) method: String,
    pub(super) redirect_uris: Vec<String>,
}

pub(super) struct Refresh {
    pub(super) grant: u64,
    pub(super) current: bool,
    pub(super) resource: Option<String>,
    pub(super) scope: String,
}

#[derive(Default)]
pub(super) struct Inner {
    pub(super) settings: Settings,
    pub(super) codes: HashMap<String, Code>,
    access: HashMap<String, (u64, Option<String>, Option<Instant>)>,
    pub(super) refresh: HashMap<String, Refresh>,
    pub(super) revoked: Vec<u64>,
    pub(super) clients: HashMap<String, Client>,
    pub(super) next: u64,
    pub(super) requests: Vec<AuthRequest>,
    /// How many times its metadata was asked for.
    pub(super) metadata_asked: usize,
}

/// The server, served at `issuer` until the process ends.
#[derive(Clone)]
pub struct FakeAuthorizationServer {
    pub issuer: String,
    pub(super) inner: Arc<Mutex<Inner>>,
}

impl FakeAuthorizationServer {
    pub async fn start() -> Self {
        Self::start_on("127.0.0.1:0").await
    }

    /// The server, listening at `address` (end-to-end tests: a known port).
    pub async fn start_on(address: &str) -> Self {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .expect("local port");
        let issuer = format!("http://{}", listener.local_addr().expect("address"));
        let server = Self {
            issuer,
            inner: Arc::default(),
        };
        let router = Router::new()
            .route(
                "/.well-known/oauth-authorization-server",
                get(metadata_endpoint::metadata),
            )
            .route("/authorize", get(authorize))
            .route("/token", post(token_endpoint::token))
            .route("/revoke", post(revocation_endpoint::revoke))
            .route("/register", post(registration_endpoint::register))
            .route(
                "/.well-known/openid-configuration",
                get(openid::configuration),
            )
            .route("/jwks", get(openid::keys))
            .with_state(server.clone());
        tokio::spawn(async move { axum::serve(listener, router).await.expect("test server") });
        server
    }

    /// Changes how the server behaves.
    pub fn set(&self, change: impl FnOnce(&mut Settings)) {
        change(&mut self.inner.lock().expect("server").settings);
    }

    /// The requests received so far.
    pub fn requests(&self) -> Vec<AuthRequest> {
        self.inner.lock().expect("server").requests.clone()
    }

    /// A token for `resource`, as if a grant had just been made.
    pub fn issue(&self, resource: &str) -> String {
        let mut inner = self.inner.lock().expect("server");
        inner.next += 1;
        let grant = inner.next;
        issue_access(&mut inner, grant, Some(resource.to_string()))
    }

    /// Whether `token` is good for `resource` now.
    pub fn accepts(&self, token: &str, resource: &str) -> bool {
        let inner = self.inner.lock().expect("server");
        inner
            .access
            .get(token)
            .is_some_and(|(grant, issued_for, expires)| {
                !inner.revoked.contains(grant)
                    && issued_for.as_deref() == Some(resource)
                    && expires.is_none_or(|at| Instant::now() < at)
            })
    }

    /// The browser sent to `url`, one of this server's authorization
    /// addresses: where it is sent back.
    pub async fn sent_back(&self, url: &str) -> String {
        let policy = NetworkPolicy {
            allow_private_network: true,
            ..NetworkPolicy::default()
        };
        let answer = http_client(policy)
            .expect("client")
            .get(url)
            .send()
            .await
            .expect("answer");
        answer
            .headers()
            .get(reqwest_mcp::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .expect("sent back")
            .to_string()
    }

    /// How many times its metadata was asked for (RFC 8414).
    pub fn metadata_asked(&self) -> usize {
        self.inner.lock().expect("server").metadata_asked
    }

    /// The clients registered so far.
    pub fn registered(&self) -> Vec<String> {
        let inner = self.inner.lock().expect("server");
        inner.clients.keys().cloned().collect()
    }

    /// Revokes every grant made so far, as an administrator at the
    /// authorization server would.
    pub fn revoke_all(&self) {
        let mut inner = self.inner.lock().expect("server");
        let grants = 1..=inner.next;
        inner.revoked.extend(grants);
    }
}

impl Inner {
    /// The grant a token it issued, access or refresh, belongs to.
    pub(super) fn grant_of(&self, token: &str) -> Option<u64> {
        let access = self.access.get(token).map(|(grant, ..)| *grant);
        access.or_else(|| self.refresh.get(token).map(|refresh| refresh.grant))
    }

    /// Whether `token` is an access token it issued.
    pub(super) fn issued_access(&self, token: &str) -> bool {
        self.access.contains_key(token)
    }
}

pub(super) fn token_value(inner: &mut Inner, kind: &str) -> String {
    inner.next += 1;
    let mark = if inner.settings.off_alphabet {
        "é"
    } else {
        ""
    };
    format!("{kind}-{}-{:016x}{mark}", inner.next, rand_part(inner.next))
}

fn rand_part(seed: u64) -> u64 {
    seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x5851_f42d_4c95_7f2d
}

/// How long an access token is with `long_tokens`: more than a typed
/// token may be (`MAX_TOKEN_CHARS`).
pub const LONG_TOKEN_CHARS: usize = 2500;

pub(super) fn issue_access(inner: &mut Inner, grant: u64, resource: Option<String>) -> String {
    let mut token = token_value(inner, "acces");
    if inner.settings.long_tokens {
        let pad = LONG_TOKEN_CHARS.saturating_sub(token.len() + 1);
        token = format!("{token}.{}", "x".repeat(pad));
    }
    let expires = inner
        .settings
        .lifetime
        .map(|s| Instant::now() + Duration::from_secs(s));
    inner
        .access
        .insert(token.clone(), (grant, resource, expires));
    token
}

async fn authorize(
    State(server): State<FakeAuthorizationServer>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let mut inner = server.inner.lock().expect("server");
    inner.requests.push(AuthRequest {
        endpoint: "authorize",
        params: params.clone(),
        basic: None,
    });
    let field = |name: &str| params.get(name).cloned().unwrap_or_default();
    // The test client, or a registered one at a return address it gave.
    let known = field("client_id") == CLIENT_ID
        || inner
            .clients
            .get(&field("client_id"))
            .is_some_and(|client| {
                client
                    .redirect_uris
                    .iter()
                    .any(|uri| *uri == field("redirect_uri"))
            });
    if field("response_type") != "code"
        || !known
        || field("code_challenge_method") != "S256"
        || field("code_challenge").is_empty()
        || field("redirect_uri").is_empty()
    {
        return (StatusCode::BAD_REQUEST, "invalid_request").into_response();
    }
    let code = token_value(&mut inner, "code");
    inner.codes.insert(
        code.clone(),
        Code {
            redirect_uri: field("redirect_uri"),
            challenge: field("code_challenge"),
            resource: params.get("resource").cloned(),
            scope: field("scope"),
            client_id: field("client_id"),
            nonce: params.get("nonce").cloned(),
        },
    );
    let Ok(mut back) = url::Url::parse(&field("redirect_uri")) else {
        return (StatusCode::BAD_REQUEST, "invalid_request").into_response();
    };
    back.query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &field("state"));
    if inner.settings.iss_on_return {
        back.query_pairs_mut().append_pair("iss", &server.issuer);
    }
    Redirect::to(back.as_str()).into_response()
}
