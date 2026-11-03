//! An OAuth authorization server, as an MCP server's would be: its metadata
//! (RFC 8414), an authorization endpoint answering at once (no one to sign
//! in, the code sent back to the redirect address), and a token endpoint
//! checking the client, PKCE S256 and the resource (RFC 8707), renewing
//! with rotation and revoking the whole grant when a rotated refresh token
//! is used again, as careful servers do. Every request is recorded; the
//! tests turn its behaviour (`Settings`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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

struct Code {
    redirect_uri: String,
    challenge: String,
    resource: Option<String>,
    scope: String,
}

struct Refresh {
    grant: u64,
    current: bool,
    resource: Option<String>,
    scope: String,
}

#[derive(Default)]
struct Inner {
    settings: Settings,
    codes: HashMap<String, Code>,
    access: HashMap<String, (u64, Option<String>, Option<Instant>)>,
    refresh: HashMap<String, Refresh>,
    revoked: Vec<u64>,
    next: u64,
    requests: Vec<AuthRequest>,
}

/// The server, served at `issuer` until the process ends.
#[derive(Clone)]
pub struct FakeAuthorizationServer {
    pub issuer: String,
    inner: Arc<Mutex<Inner>>,
}

impl FakeAuthorizationServer {
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
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
            .route("/token", post(token))
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

    /// Revokes every grant made so far, as an administrator at the
    /// authorization server would.
    pub fn revoke_all(&self) {
        let mut inner = self.inner.lock().expect("server");
        let grants = 1..=inner.next;
        inner.revoked.extend(grants);
    }
}

fn token_value(inner: &mut Inner, kind: &str) -> String {
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

fn issue_access(inner: &mut Inner, grant: u64, resource: Option<String>) -> String {
    let token = token_value(inner, "acces");
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
    let mut back = url::Url::parse(&field("redirect_uri")).expect("redirect address");
    back.query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &field("state"));
    if inner.settings.iss_on_return {
        back.query_pairs_mut().append_pair("iss", &server.issuer);
    }
    Redirect::to(back.as_str()).into_response()
}

async fn token(
    State(server): State<FakeAuthorizationServer>,
    headers: HeaderMap,
    Form(params): Form<HashMap<String, String>>,
) -> Response {
    let mut inner = server.inner.lock().expect("server");
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| STANDARD.decode(v).ok())
        .and_then(|v| String::from_utf8(v).ok());
    inner.requests.push(AuthRequest {
        endpoint: "token",
        params: params.clone(),
        basic: basic.clone(),
    });
    let field = |name: &str| params.get(name).cloned().unwrap_or_default();
    let (client, secret) = match &basic {
        Some(pair) => match pair.split_once(':') {
            Some((id, secret)) => (id.to_string(), Some(secret.to_string())),
            None => return refused("invalid_client"),
        },
        None => (field("client_id"), params.get("client_secret").cloned()),
    };
    if client != CLIENT_ID || secret.as_deref().is_some_and(|s| s != CLIENT_SECRET) {
        return refused("invalid_client");
    }
    match field("grant_type").as_str() {
        "authorization_code" => {
            let Some(code) = inner.codes.remove(&field("code")) else {
                return refused("invalid_grant");
            };
            let verified =
                URL_SAFE_NO_PAD.encode(Sha256::digest(field("code_verifier").as_bytes()));
            if code.redirect_uri != field("redirect_uri")
                || verified != code.challenge
                || code.resource != params.get("resource").cloned()
            {
                return refused("invalid_grant");
            }
            inner.next += 1;
            let grant = inner.next;
            granted(&mut inner, grant, code.resource, code.scope)
        }
        "refresh_token" => {
            let given = field("refresh_token");
            let Some(refresh) = inner.refresh.get_mut(&given) else {
                return refused("invalid_grant");
            };
            let (grant, current) = (refresh.grant, refresh.current);
            let (resource, scope) = (refresh.resource.clone(), refresh.scope.clone());
            refresh.current = false;
            if !current {
                // A rotated token used again: the whole grant is revoked.
                inner.revoked.push(grant);
            }
            if !current
                || inner.revoked.contains(&grant)
                || resource != params.get("resource").cloned()
            {
                return refused("invalid_grant");
            }
            granted(&mut inner, grant, resource, scope)
        }
        _ => refused("unsupported_grant_type"),
    }
}

fn granted(inner: &mut Inner, grant: u64, resource: Option<String>, scope: String) -> Response {
    let access = issue_access(inner, grant, resource.clone());
    let mut body = json!({ "access_token": access, "token_type": "Bearer", "scope": scope });
    if let Some(lifetime) = inner.settings.lifetime {
        body["expires_in"] = Value::from(lifetime);
    }
    if inner.settings.refresh {
        let refresh = token_value(inner, "renouvellement");
        inner.refresh.insert(
            refresh.clone(),
            Refresh {
                grant,
                current: true,
                resource,
                scope,
            },
        );
        body["refresh_token"] = Value::from(refresh);
    }
    axum::Json(body).into_response()
}

fn refused(error: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        axum::Json(json!({ "error": error })),
    )
        .into_response()
}
