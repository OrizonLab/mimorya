//! The tokens of an MCP server's authorization revoked at its authorization
//! server once the administration lets go of them (RFC 7009, ADR-172): when
//! the authorization is withdrawn, the declaration withdrawn or declared
//! again, the server's address changed or the connector deleted; the
//! refresh token first, under the declaration it was given under, its
//! client's secret read before it goes, never under another declaration.
//! The withdrawal stands whatever the
//! server does: one naming no revocation endpoint, or not answering in
//! time, is audited as such, never with a token. A new authorization does
//! not revoke the former: at some servers, that would revoke the new one.

mod support;

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use mimorya_connectors::mcp::oauth::NO_REVOCATION_ENDPOINT;
use mimorya_connectors::testing::{
    CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http,
};
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::connector_oauth::{
    AUTHORIZED, authorization_entries, authorize, declared, kept_tokens,
};
use support::connector_sessions::alter;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::sso::location;

/// The revocations audited for this connector: outcome and reason.
async fn revocations(api: &TestApi, id: &str) -> Vec<(String, Option<String>)> {
    authorization_entries(api, id)
        .await
        .into_iter()
        .filter(|(action, ..)| action == "connector.oauth_authorization_revoked")
        .map(|(_, outcome, reason, _)| (outcome, reason))
        .collect()
}

/// How many revocations the authorization server was asked for.
fn asked(authority: &FakeAuthorizationServer) -> usize {
    let requests = authority.requests();
    requests.iter().filter(|r| r.endpoint == "revoke").count()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_withdrawn_authorization_is_revoked_at_its_server() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "retire").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let (access, refresh) = kept_tokens(&api, &id).await.expect("kept");
    let refresh = refresh.expect("renewable");
    assert_eq!(listed(&paul, &id).await["mcp"]["oauth"]["revocable"], true);
    let path = format!("/admin/connectors/{id}/oauth/authorization");
    assert_eq!(paul.delete(&path).await.status, StatusCode::OK);
    // Asked as the client, the refresh token first; gone there.
    let requests = authority.requests();
    let revoked: Vec<_> = requests.iter().filter(|r| r.endpoint == "revoke").collect();
    assert_eq!(revoked.len(), 2);
    assert_eq!(revoked[0].params["token"], refresh);
    assert_eq!(revoked[0].params["client_secret"], CLIENT_SECRET);
    assert!(!authority.accepts(&access, &server.url));
    // Audited, without a token.
    let audited = revocations(&api, &id).await;
    assert_eq!(audited, vec![("success".to_string(), None)]);
    let all = format!("{:?}", authorization_entries(&api, &id).await);
    assert!(!all.contains(&access) && !all.contains(&refresh));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn every_way_the_administration_lets_go_of_it_revokes_it() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "facons").await;
    let oauth = format!("/admin/connectors/{id}/oauth");
    let instance = format!("/admin/connectors/{id}");
    let secret = json!({ "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET });
    // Each way, the authorization given just before is revoked there.
    let authorized = async || {
        assert_eq!(
            location(&authorize(&paul, &authority, &id).await),
            AUTHORIZED
        );
        kept_tokens(&api, &id).await.expect("kept").0
    };
    // Declared again, even as a public client: revoked with the former
    // secret.
    let access = authorized().await;
    let again = paul.put(&oauth, json!({ "clientId": CLIENT_ID })).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    assert!(!authority.accepts(&access, &server.url));
    let requests = authority.requests();
    let last = requests
        .iter()
        .rev()
        .find(|r| r.endpoint == "revoke")
        .expect("asked");
    assert_eq!(last.params["client_secret"], CLIENT_SECRET);
    // The declaration withdrawn.
    assert_eq!(
        paul.put(&oauth, secret.clone()).await.status,
        StatusCode::OK
    );
    let access = authorized().await;
    assert_eq!(paul.delete(&oauth).await.status, StatusCode::OK);
    assert!(!authority.accepts(&access, &server.url));
    // The server's address changed.
    assert_eq!(
        paul.put(&oauth, secret.clone()).await.status,
        StatusCode::OK
    );
    let access = authorized().await;
    let elsewhere = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let moved = paul.patch(&instance, json!({ "url": elsewhere.url })).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    assert!(!authority.accepts(&access, &server.url));
    // The connector deleted.
    let moved_oauth = paul.put(&oauth, secret).await;
    assert_eq!(moved_oauth.status, StatusCode::OK, "{}", moved_oauth.text());
    let access = authorized().await;
    assert_eq!(paul.delete(&instance).await.status, StatusCode::OK);
    assert!(!authority.accepts(&access, &elsewhere.url));
    let audited = revocations(&api, &id).await;
    assert_eq!(audited.len(), 4, "{audited:?}");
    assert!(
        audited.iter().all(|(outcome, _)| outcome == "success"),
        "{audited:?}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_server_that_cannot_revoke_keeps_the_withdrawal_and_says_so() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    // Naming no revocation endpoint when it was declared.
    let silent = FakeAuthorizationServer::start().await;
    silent.set(|settings| settings.revocation = false);
    let (server, id) = declared(&paul, &silent, "sans").await;
    assert_eq!(location(&authorize(&paul, &silent, &id).await), AUTHORIZED);
    let (access, _) = kept_tokens(&api, &id).await.expect("kept");
    assert_eq!(listed(&paul, &id).await["mcp"]["oauth"]["revocable"], false);
    let path = format!("/admin/connectors/{id}/oauth/authorization");
    assert_eq!(paul.delete(&path).await.status, StatusCode::OK);
    assert!(kept_tokens(&api, &id).await.is_none());
    assert_eq!(asked(&silent), 0);
    assert!(silent.accepts(&access, &server.url));
    let audited = revocations(&api, &id).await;
    let said = NO_REVOCATION_ENDPOINT.to_string();
    assert_eq!(audited, vec![("failure".to_string(), Some(said))]);
    // Not answering: waited for a bound, the withdrawal kept.
    let mute = FakeAuthorizationServer::start().await;
    mute.set(|settings| settings.mute_revocation = true);
    let (_, other) = declared(&paul, &mute, "muet").await;
    assert_eq!(location(&authorize(&paul, &mute, &other).await), AUTHORIZED);
    let path = format!("/admin/connectors/{other}/oauth/authorization");
    let started = Instant::now();
    assert_eq!(paul.delete(&path).await.status, StatusCode::OK);
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "{:?}",
        started.elapsed()
    );
    assert!(kept_tokens(&api, &other).await.is_none());
    let audited = revocations(&api, &other).await;
    let (outcome, reason) = audited.last().expect("audited");
    assert_eq!(outcome, "failure");
    assert!(
        reason.as_deref().unwrap_or_default().contains("à temps"),
        "{reason:?}"
    );
    for connector in [&id, &other] {
        let deleted = paul.delete(&format!("/admin/connectors/{connector}")).await;
        assert_eq!(deleted.status, StatusCode::OK);
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn tokens_kept_under_another_declaration_are_never_sent_under_this_one() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "ailleurs").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let elsewhere = "update connector_authorizations set issuer = \
                     'https://autre-emetteur.example' where connector_id = $1";
    alter(&api, &id, elsewhere).await;
    let path = format!("/admin/connectors/{id}/oauth/authorization");
    assert_eq!(paul.delete(&path).await.status, StatusCode::OK);
    assert_eq!(asked(&authority), 0);
    let audited = revocations(&api, &id).await;
    let (outcome, reason) = audited.last().expect("audited");
    assert_eq!(outcome, "failure");
    assert!(
        reason
            .as_deref()
            .unwrap_or_default()
            .contains("autre déclaration"),
        "{reason:?}"
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_new_authorization_does_not_revoke_the_former() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "deux").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let (first, _) = kept_tokens(&api, &id).await.expect("kept");
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    assert_eq!(asked(&authority), 0);
    assert!(authority.accepts(&first, &server.url));
    assert!(revocations(&api, &id).await.is_empty());
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
