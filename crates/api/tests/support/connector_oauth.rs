//! An MCP server's OAuth in the tests (ADR-171): a server declared at the
//! simulated authorization server, its authorization started, the browser
//! at the authorization server and back, and what the database and the
//! audit keep.

use axum::http::StatusCode;
use mimorya_api::services::admin::{access_sealed_as, refresh_sealed_as};
use mimorya_connectors::testing::{
    CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, OAuthQuirk, ProtectedMcp,
    serve_oauth_mcp_http,
};
use mimorya_core::demo::tenant;
use mimorya_db::repositories::connector_authorizations::sealed_connector_tokens;
use serde_json::{Value, json};

use super::WEB_ORIGIN;
use super::mcp::{add, mcp_body};
use super::sso::set_cookie;
use super::{Client, TestApi, TestResponse};

pub const AUTHORIZED: &str = "/admin?autorisation=ok";
pub const FAILED: &str = "/admin?autorisation=echec";
pub const COOKIE: &str = "mimorya_connector_oauth";

/// A server protected by `authority`, added and declared with OAuth (its
/// client with its secret): the server and the connector's id.
pub async fn declared(
    admin: &Client<'_>,
    authority: &FakeAuthorizationServer,
    prefix: &str,
) -> (ProtectedMcp, String) {
    let server = serve_oauth_mcp_http(authority, OAuthQuirk::None).await;
    let name = format!("Fichiers {prefix}");
    let id = add(admin, mcp_body(&name, prefix, &server.url, None)).await;
    let declared = admin
        .put(
            &format!("/admin/connectors/{id}/oauth"),
            json!({ "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET }),
        )
        .await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    (server, id)
}

pub async fn try_start(admin: &Client<'_>, id: &str) -> TestResponse {
    admin
        .post(
            &format!("/admin/connectors/{id}/oauth/authorization"),
            json!({}),
        )
        .await
}

/// The authorization server's address and the attempt's cookie
/// (`name=value`).
pub async fn start(admin: &Client<'_>, id: &str) -> (String, String) {
    let started = try_start(admin, id).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let cookie = set_cookie(&started, COOKIE).expect("attempt cookie");
    assert!(cookie.contains("HttpOnly") && cookie.contains("Path=/api/admin/connectors/oauth"));
    let pair = cookie.split(';').next().unwrap().to_string();
    let url = started.json()["redirectUrl"].as_str().unwrap().to_string();
    (url, pair)
}

/// The return's path as the test client asks it: the browser's address
/// under the web origin, without the API's prefix.
pub fn api_path(location: &str) -> String {
    location
        .strip_prefix(&format!("{WEB_ORIGIN}/api"))
        .expect("under the API")
        .to_string()
}

/// The browser's return, signed in as `admin`, with this cookie, if any.
pub async fn come_back(admin: &Client<'_>, path: &str, cookie: Option<&str>) -> TestResponse {
    match cookie {
        Some(cookie) => admin.get_with(path, &[("cookie", cookie)]).await,
        None => admin.get(path).await,
    }
}

/// From the start to the return, the authorization server agreeing.
pub async fn authorize(
    admin: &Client<'_>,
    authority: &FakeAuthorizationServer,
    id: &str,
) -> TestResponse {
    let (url, cookie) = start(admin, id).await;
    let back = api_path(&authority.sent_back(&url).await);
    come_back(admin, &back, Some(&cookie)).await
}

/// The tokens the connector keeps, opened as the server opens them.
pub async fn kept_tokens(api: &TestApi, id: &str) -> Option<(String, Option<String>)> {
    let connector = id.parse().expect("id");
    let services = &api.app.services;
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    let sealed = sealed_connector_tokens(&mut tx, connector).await.unwrap();
    tx.commit().await.unwrap();
    let (access, refresh) = sealed?;
    let cipher = services.config.secrets.clone().expect("master key");
    let access = cipher
        .open(tenant::ID, &access_sealed_as(connector), &access)
        .expect("sealed for the connector");
    let refresh = refresh.map(|refresh| {
        cipher
            .open(tenant::ID, &refresh_sealed_as(connector), &refresh)
            .expect("sealed for the connector")
    });
    Some((access, refresh))
}

/// The audit entries of this connector's authorizations: action, outcome,
/// reason and metadata.
pub async fn authorization_entries(
    api: &TestApi,
    id: &str,
) -> Vec<(String, String, Option<String>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_as(
        "select action, outcome, reason, metadata from audit_log \
         where resource_id = $1 and action like 'connector.oauth_authori%' order by id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}
