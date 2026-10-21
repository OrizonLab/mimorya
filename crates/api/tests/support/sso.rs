//! Signing in through a company's identity provider (ADR-063) in the
//! tests: the provider declared by Paul, the start, the browser's visit to
//! the provider and its return, and what the audit and the identities say.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mimorya_core::demo::tenant;
use serde_json::{Value, json};
use uuid::Uuid;

use super::identity_provider::{CLIENT_ID, CLIENT_SECRET, MockIdentityProvider};
use super::llm::MASTER_KEY;
use super::{Client, TestApi, TestResponse, WEB_ORIGIN};

/// A server storing secrets and reaching the local provider.
pub const SSO_DEPLOYMENT: [(&str, &str); 2] = [
    ("MIMORYA_MASTER_KEY", MASTER_KEY),
    ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true"),
];
pub const DOMAIN: &str = "neo-maintenance.example";
pub const PROVIDERS: &str = "/admin/identity-providers";
pub const REFUSED: &str = "/login?sso=echec";

/// A generic provider of the demo company's domain, at the local provider.
pub fn declaration(idp: &MockIdentityProvider) -> Value {
    json!({
        "kind": "oidc",
        "name": "Annuaire Néo",
        "issuer": idp.issuer,
        "clientId": CLIENT_ID,
        "clientSecret": CLIENT_SECRET,
        "domains": [DOMAIN],
    })
}

/// Declares it and returns its id.
pub async fn declare(admin: &Client<'_>, body: Value) -> String {
    let created = admin.post(PROVIDERS, body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_string()
}

pub async fn remove(admin: &Client<'_>, id: &str) {
    let removed = admin.delete(&format!("{PROVIDERS}/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
}

async fn anonymous_post(api: &TestApi, url: &str, payload: Value) -> TestResponse {
    let request = Request::post(url)
        .header("x-mimorya-request", "1")
        .header("origin", WEB_ORIGIN)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    api.send(request).await
}

/// What the sign-in page is offered for an address.
pub async fn discover(api: &TestApi, email: &str) -> Value {
    let found = anonymous_post(api, "/auth/sso/discover", json!({ "email": email })).await;
    assert_eq!(found.status, StatusCode::OK, "{}", found.text());
    found.json()["provider"].clone()
}

pub async fn try_start(api: &TestApi, email: &str) -> TestResponse {
    anonymous_post(api, "/auth/sso/start", json!({ "email": email })).await
}

/// The provider's address and the attempt's cookie (`name=value`).
pub async fn start(api: &TestApi, email: &str) -> (String, String) {
    let started = try_start(api, email).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let cookie = set_cookie(&started, "mimorya_sso").expect("attempt cookie");
    assert!(cookie.contains("HttpOnly") && cookie.contains("Path=/api/auth/sso"));
    let pair = cookie.split(';').next().unwrap().to_string();
    let url = started.json()["redirectUrl"].as_str().unwrap().to_string();
    (url, pair)
}

/// The browser's return from the provider, with this cookie, if any.
pub async fn come_back(api: &TestApi, path: &str, cookie: Option<&str>) -> TestResponse {
    let mut request = Request::get(path);
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    api.send(request.body(Body::empty()).unwrap()).await
}

/// From the start to the return, signed in at the provider as it was told.
pub async fn sign_in(api: &TestApi, idp: &MockIdentityProvider, email: &str) -> TestResponse {
    let (url, cookie) = start(api, email).await;
    let back = idp.authorize(&url);
    come_back(api, &back, Some(&cookie)).await
}

/// The `Set-Cookie` header naming this cookie.
pub fn set_cookie(response: &TestResponse, name: &str) -> Option<String> {
    response
        .headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with(&format!("{name}=")))
        .map(String::from)
}

/// Where the return sends the browser.
pub fn location(response: &TestResponse) -> &str {
    assert_eq!(
        response.status,
        StatusCode::SEE_OTHER,
        "{}",
        response.text()
    );
    response.headers["location"].to_str().unwrap()
}

/// The session the return opened; it also clears the attempt.
pub fn session(response: &TestResponse) -> Option<String> {
    let cleared = set_cookie(response, "mimorya_sso").expect("attempt cleared");
    assert!(cleared.contains("Max-Age=0"), "{cleared}");
    set_cookie(response, "mimorya_session").map(|cookie| {
        cookie
            .strip_prefix("mimorya_session=")
            .and_then(|rest| rest.split(';').next())
            .unwrap()
            .to_string()
    })
}

/// A refused return: back to the sign-in page, no session.
pub fn assert_refused(response: &TestResponse) {
    assert_eq!(location(response), REFUSED);
    assert_eq!(session(response), None);
}

/// The single sign-on attempts audited in a company after `since`:
/// outcome, account, reason.
pub async fn sso_logins(
    api: &TestApi,
    tenant_id: Uuid,
    since: i64,
) -> Vec<(String, Option<Uuid>, Option<String>)> {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query_as(
        "select outcome, actor_user_id, reason from audit_log \
         where id > $1 and action = 'auth.login' and metadata->>'method' = 'sso' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// The subjects tied to the demo company's accounts: account, subject.
pub async fn identities(api: &TestApi) -> Vec<(Uuid, String)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as("select user_id, subject from user_identities order by created_at")
        .fetch_all(&mut *tx)
        .await
        .unwrap()
}
