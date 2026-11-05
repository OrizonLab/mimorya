//! What changes while a person's code is being exchanged (ADR-174): their
//! account disabled, or the server's declaration changed, the tokens
//! granted are kept nowhere, the reason audited, and revoked at the
//! authorization server; unless the same person already holds an
//! authorization of the same client there, which revoking them could take
//! with them. Checked under the administration lock, after the exchange.

mod support;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::accounts::{activate, create_account};
use support::connector_oauth::{api_path, come_back};
use support::connector_sessions::{alter, token_endpoint};
use support::held_token::held;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{
    PERSON_FAILED, declared_per_person, entries_by, person_authorized, person_kept, person_start,
};
use support::sso::location;
use support::{Client, TestApi};

const AUTHORIZED: &str = "connector.person_oauth_authorized";

/// The tokens the authorization server was asked to revoke, waited for a
/// while: they are revoked by a task of their own.
async fn revoked(authority: &FakeAuthorizationServer, expected: usize) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let tokens: Vec<String> = authority
            .requests()
            .into_iter()
            .filter(|r| r.endpoint == "revoke")
            .filter_map(|r| r.params.get("token").cloned())
            .collect();
        if tokens.len() >= expected || Instant::now() > deadline {
            return tokens;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A server each person authorizes whose token endpoint holds its answer.
async fn held_server(
    paul: &Client<'_>,
    api: &TestApi,
    authority: &FakeAuthorizationServer,
    prefix: &str,
) -> (
    String,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::Arc<tokio::sync::Notify>,
) {
    let (_, id) = declared_per_person(paul, authority, prefix).await;
    let (endpoint, read, release) = held().await;
    alter(api, &id, &token_endpoint(&endpoint)).await;
    (id, read, release)
}

/// `person`'s return, `meanwhile` done while their code is exchanged.
async fn returned_while(
    person: &Client<'_>,
    authority: &FakeAuthorizationServer,
    id: &str,
    read: &std::sync::atomic::AtomicUsize,
    release: &tokio::sync::Notify,
    meanwhile: impl std::future::Future<Output = ()>,
) -> String {
    let (url, cookie) = person_start(person, id).await;
    let back = api_path(&authority.sent_back(&url).await);
    let returned = come_back(person, &back, Some(&cookie));
    let changed = async {
        while read.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        meanwhile.await;
        release.notify_one();
    };
    let (returned, ()) = tokio::join!(returned, changed);
    location(&returned).to_string()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_disabled_during_the_exchange_keeps_nothing() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (id, read, release) = held_server(&paul, &api, &authority, "desactive").await;
    let mut account = create_account(&paul, "course.desactive").await;
    let leaving = activate(&api, &mut account).await;
    let disable = async {
        let disabled = paul
            .patch(
                &format!("/admin/users/{}", account.id),
                json!({ "status": "disabled" }),
            )
            .await;
        assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    };
    let back = returned_while(&leaving, &authority, &id, &read, &release, disable).await;
    assert_eq!(back, PERSON_FAILED);
    assert!(person_kept(&api, &id, account.id).await.is_none());
    let reason = entries_by(&api, Some(&id), account.id, AUTHORIZED).await;
    assert_eq!(
        reason.last().unwrap().1.as_deref(),
        Some("Compte désactivé pendant l’autorisation")
    );
    // Granted, kept nowhere: revoked, the refresh token first.
    let tokens = revoked(&authority, 2).await;
    assert_eq!(tokens, ["renouvellement-retenu", "jeton-retenu"]);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_declaration_changed_during_the_exchange_keeps_nothing() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (id, read, release) = held_server(&paul, &api, &authority, "redeclare").await;
    let holder = format!("/admin/connectors/{id}/oauth/holder");
    let to_company = async {
        let changed = paul.put(&holder, json!({ "perPerson": false })).await;
        assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    };
    let back = returned_while(&marc, &authority, &id, &read, &release, to_company).await;
    assert_eq!(back, PERSON_FAILED);
    assert!(person_kept(&api, &id, user::MARC).await.is_none());
    let reason = entries_by(&api, Some(&id), user::MARC, AUTHORIZED).await;
    let reason = reason.last().unwrap().1.clone().unwrap_or_default();
    assert!(
        reason.contains("changée pendant l’autorisation"),
        "{reason}"
    );
    let tokens = revoked(&authority, 2).await;
    assert_eq!(tokens, ["renouvellement-retenu", "jeton-retenu"]);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_refused_return_leaves_what_the_same_person_holds() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (id, read, release) = held_server(&paul, &api, &authority, "garde").await;
    // Marc already holds an authorization of the same client there.
    let oauth: Value = listed(&paul, &id).await["mcp"]["oauth"].clone();
    let resource = oauth["resource"].as_str().unwrap().to_string();
    let held_token = person_authorized(&api, &id, user::MARC, &authority, &resource).await;
    let disable_server = async {
        let off = paul
            .patch(
                &format!("/admin/connectors/{id}"),
                json!({ "enabled": false }),
            )
            .await;
        assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    };
    let back = returned_while(&marc, &authority, &id, &read, &release, disable_server).await;
    assert_eq!(back, PERSON_FAILED);
    // Refused, nothing revoked: the authorization Marc holds stays good.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(revoked(&authority, 0).await.is_empty());
    assert!(authority.accepts(&held_token, &resource));
    assert_eq!(
        person_kept(&api, &id, user::MARC).await.as_deref(),
        Some(held_token.as_str())
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
