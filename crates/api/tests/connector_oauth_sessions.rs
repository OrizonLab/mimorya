//! The sessions of an MCP server authorized with OAuth (ADR-171): runs and
//! the administration's test are given its tools through the access token
//! of its authorization, sent to the server alone; not authorized, its
//! authorization withdrawn or issued under another declaration, it says to
//! authorize it; a token about to expire, one that cannot be opened, or one
//! the server refuses, says to authorize it again, the token never shown.
//! A process that missed the notice of a change still follows it: the
//! authorization is part of the connector's version.

mod support;

use axum::http::StatusCode;
use mimorya_api::secrets::SecretStore;
use mimorya_api::services::connectors::{ConnectorService, ConnectorSettings};
use mimorya_api::services::invalidations::Invalidations;
use mimorya_connectors::mcp::oauth::OAUTH_TO_AUTHORIZE;
use mimorya_connectors::mcp::{OAUTH_EXPIRED, OAUTH_REFUSED};
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::connector_oauth::{AUTHORIZED, authorize, declared, kept_tokens};
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::sso::location;
use support::{Client, TestApi};
use uuid::Uuid;

/// The administration's test of the connector: its tools, or why not.
async fn tested(admin: &Client<'_>, id: &str) -> Value {
    let tested = admin
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await;
    assert_eq!(tested.status, StatusCode::OK, "{}", tested.text());
    tested.json()
}

fn said(outcome: &Value) -> String {
    outcome["error"].as_str().unwrap_or_default().to_string()
}

/// Whether runs are given the server's tools (prefixed with `prefix`).
async fn runs_see(api: &TestApi, prefix: &str) -> bool {
    sees(&api.app.services.connectors, prefix).await
}

/// Whether the runs of the process holding `connectors` are given them.
async fn sees(connectors: &ConnectorService, prefix: &str) -> bool {
    let tools = connectors.tools_for(tenant::ID).await.unwrap();
    let namespace = format!("mcp_{prefix}_");
    tools
        .tools
        .iter()
        .any(|tool| tool.info().name.starts_with(&namespace))
}

/// Another process's connectors, which hear none of this one's notices.
fn another_process(api: &TestApi) -> ConnectorService {
    let services = &api.app.services;
    let config = &services.config;
    let settings = ConnectorSettings {
        network: config.mcp_network,
        start_timeout: config.mcp_start_timeout,
    };
    let secrets = SecretStore::new(config.secrets.clone());
    ConnectorService::new(
        services.db.clone(),
        secrets,
        settings,
        Invalidations::new().0,
    )
}

/// Changes the stored authorization as an operator could, then releases the
/// live instance.
async fn alter(api: &TestApi, id: &str, sql: &str) {
    let id: Uuid = id.parse().unwrap();
    let services = &api.app.services;
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(sql).bind(id).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    services.connectors.invalidate(id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_authorized_server_serves_its_tools_with_its_authorization_s_token() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "sessions").await;
    // Declared, not authorized: it says so, runs go without it.
    let before = tested(&paul, &id).await;
    assert_eq!(before["ok"], false);
    assert!(said(&before).contains(OAUTH_TO_AUTHORIZE), "{before}");
    assert!(!runs_see(&api, "sessions").await);
    let asked = server.authorizations().len();

    // Authorized: its tools, for the administration's test and for runs.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let (access, _) = kept_tokens(&api, &id).await.expect("kept");
    let after = tested(&paul, &id).await;
    assert_eq!(after["ok"], true, "{after}");
    assert!(!after["tools"].as_array().unwrap().is_empty());
    assert!(runs_see(&api, "sessions").await);
    // Every request since with the authorization's token.
    let bearer = format!("Bearer {access}");
    let since = server.authorizations().split_off(asked);
    assert!(!since.is_empty());
    assert!(
        since
            .iter()
            .all(|given| given.as_deref() == Some(bearer.as_str()))
    );
    assert_eq!(listed(&paul, &id).await["status"], "ok");

    // Withdrawn: the live session released, runs go without it.
    let authorization = format!("/admin/connectors/{id}/oauth/authorization");
    assert_eq!(paul.delete(&authorization).await.status, StatusCode::OK);
    assert!(!runs_see(&api, "sessions").await);
    let shown = listed(&paul, &id).await;
    assert_eq!(shown["status"], "error");
    assert!(
        shown["lastError"]
            .as_str()
            .unwrap_or_default()
            .contains(OAUTH_TO_AUTHORIZE),
        "{shown}"
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_authorization_issued_elsewhere_about_to_expire_or_refused_says_so() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "echeances").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    // Issued under another declaration (resource, issuer, client): not used.
    for elsewhere in [
        "resource = 'https://autre.example/mcp'",
        "issuer = 'https://autre-emetteur.example'",
        "client_id = 'autre-client'",
    ] {
        let sql =
            format!("update connector_authorizations set {elsewhere} where connector_id = $1");
        alter(&api, &id, &sql).await;
        let other = tested(&paul, &id).await;
        assert!(
            said(&other).contains(OAUTH_TO_AUTHORIZE),
            "{elsewhere}: {other}"
        );
        assert_eq!(
            location(&authorize(&paul, &authority, &id).await),
            AUTHORIZED
        );
    }
    // Sealed for another connector: not opened.
    let (_, other_id) = declared(&paul, &authority, "voisin").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &other_id).await),
        AUTHORIZED
    );
    let swapped = format!(
        "update connector_authorizations a set access_ciphertext = b.access_ciphertext, \
         access_iv = b.access_iv, access_auth_tag = b.access_auth_tag \
         from connector_authorizations b where a.connector_id = $1 and b.connector_id = '{other_id}'"
    );
    alter(&api, &id, &swapped).await;
    let unopened = tested(&paul, &id).await;
    assert!(
        said(&unopened).contains("ne peut pas être déchiffré"),
        "{unopened}"
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{other_id}"))
            .await
            .status,
        StatusCode::OK
    );
    // About to expire: nothing starts.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let soon = "update connector_authorizations set access_expires_at = \
                now() + interval '2 minutes' where connector_id = $1";
    alter(&api, &id, soon).await;
    let expiring = tested(&paul, &id).await;
    assert!(said(&expiring).contains(OAUTH_EXPIRED), "{expiring}");
    assert!(!runs_see(&api, "echeances").await);
    // Refused by the server (revoked there): to authorize again, the
    // token never shown.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let (access, _) = kept_tokens(&api, &id).await.expect("kept");
    authority.revoke_all();
    let refused = tested(&paul, &id).await;
    assert!(said(&refused).contains(OAUTH_REFUSED), "{refused}");
    assert!(!refused.to_string().contains(&access));
    let shown = listed(&paul, &id).await.to_string();
    assert!(!shown.contains(&access));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_process_that_missed_the_notice_follows_the_authorization() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "processus").await;
    // The worker, say, whose listening was reconnecting meanwhile: it
    // remembers that the server is to be authorized.
    let worker = another_process(&api);
    assert!(!sees(&worker, "processus").await);
    assert_eq!(listed(&paul, &id).await["status"], "error");
    // Each change of the authorization makes a new version, to be tested.
    let untested = |shown: Value| {
        assert_eq!(shown["status"], "untested", "{shown}");
        assert!(shown["lastError"].is_null(), "{shown}");
    };
    // Authorized: it starts the session at once.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    untested(listed(&paul, &id).await);
    assert!(sees(&worker, "processus").await);
    assert_eq!(listed(&paul, &id).await["status"], "ok");
    // Authorized again: its session started again with the new token.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    untested(listed(&paul, &id).await);
    let (second, _) = kept_tokens(&api, &id).await.expect("kept");
    let asked = server.authorizations().len();
    assert!(sees(&worker, "processus").await);
    let bearer = Some(format!("Bearer {second}"));
    assert!(server.authorizations()[asked..].contains(&bearer));
    // Withdrawn: it goes without it.
    let authorization = format!("/admin/connectors/{id}/oauth/authorization");
    assert_eq!(paul.delete(&authorization).await.status, StatusCode::OK);
    untested(listed(&paul, &id).await);
    assert!(!sees(&worker, "processus").await);
    worker.close().await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
