//! An OAuth authorization server, as an MCP server's would be: its metadata
//! (RFC 8414), an authorization endpoint answering at once (no one to sign
//! in, the code sent back to the redirect address), and its token endpoint
//! (`token_endpoint`). Every request is recorded; the tests turn its
//! behaviour (`Settings`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use serde_json::json;

use super::token_endpoint;
use crate::outbound::{NetworkPolicy, http_client};

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
    pub(super) next: u64,
    pub(super) requests: Vec<AuthRequest>,
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
            .route("/.well-known/oauth-authorization-server", get(metadata))
            .route("/authorize", get(authorize))
            .route("/token", post(token_endpoint::token))
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

    /// Revokes every grant made so far, as an administrator at the
    /// authorization server would.
    pub fn revoke_all(&self) {
        let mut inner = self.inner.lock().expect("server");
        let grants = 1..=inner.next;
        inner.revoked.extend(grants);
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

async fn metadata(State(server): State<FakeAuthorizationServer>) -> Response {
    let settings = server.inner.lock().expect("server").settings.clone();
    let issuer = &server.issuer;
    let named = if settings.wrong_issuer {
        format!("{issuer}/autre")
    } else {
        issuer.clone()
    };
    axum::Json(json!({
        "issuer": named,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "response_types_supported": ["code"],
        "code_challenge_methods_supported": ["S256"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_methods_supported": settings.auth_methods,
        "scopes_supported": ["files:read", "offline_access"],
        "authorization_response_iss_parameter_supported": settings.iss_on_return,
    }))
    .into_response()
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
    if field("response_type") != "code"
        || field("client_id") != CLIENT_ID
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
