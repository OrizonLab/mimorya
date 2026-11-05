//! When a person's own OAuth access token of an MCP server is renewed
//! (ADR-174): never for a run serving someone who may not use connectors,
//! nor for an evaluation, which serves no one; on a disabled server only
//! for an administrator's test, and then their own token alone. It is never
//! renewed under another declaration than the one it was given under, nor
//! where the company authorizes the server. A refresh token the platform
//! can no longer open is never sent, and the person is told.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::connectors::Served;
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::user;
use serde_json::json;
use support::connector_sessions::{alter, as_person, sees_for, tested};
use support::mcp::PRIVATE_DEPLOYMENT;
use support::person_authorizations::{
    PERSON_AUTHORIZED, declared_per_person, person_authorize, person_kept,
};
use support::person_renewals::{EXPIRING, SOON, UNCLAIMED, for_person, own_shown, renewals};
use support::sso::location;
use support::{Client, TestApi};

/// `person` authorizes the connector through their browser: real tokens,
/// renewable.
async fn authorizes(person: &Client<'_>, authority: &FakeAuthorizationServer, id: &str) {
    let back = person_authorize(person, authority, id).await;
    assert_eq!(location(&back), PERSON_AUTHORIZED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn neither_who_may_not_use_connectors_nor_an_evaluation_renews_it() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "nonrenouv").await;
    authorizes(&marc, &authority, &id).await;
    for_person(&api, &id, user::MARC, SOON).await;
    let before = person_kept(&api, &id, user::MARC).await;
    let connectors = &api.app.services.connectors;

    let barred = Served::Person {
        user: user::MARC,
        uses_connectors: false,
    };
    for served in [barred, Served::Evaluation] {
        assert!(!sees_for(connectors, "nonrenouv", served).await);
    }
    assert_eq!(renewals(&authority), 0);
    assert_eq!(person_kept(&api, &id, user::MARC).await, before);
    // Due all along: his own run renews it.
    assert!(sees_for(connectors, "nonrenouv", as_person(user::MARC)).await);
    assert_eq!(renewals(&authority), 1);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_server_is_renewed_for_its_administrator_s_own_test_alone() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "eteintsoi").await;
    for person in [&paul, &marc] {
        authorizes(person, &authority, &id).await;
    }
    let off = paul
        .patch(
            &format!("/admin/connectors/{id}"),
            json!({ "enabled": false }),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    for person in [user::PAUL, user::MARC] {
        for_person(&api, &id, person, EXPIRING).await;
    }
    let (pauls, marcs) = (
        person_kept(&api, &id, user::PAUL).await,
        person_kept(&api, &id, user::MARC).await,
    );
    // Runs leave its server alone.
    let connectors = &api.app.services.connectors;
    for person in [user::PAUL, user::MARC] {
        assert!(!sees_for(connectors, "eteintsoi", as_person(person)).await);
    }
    assert_eq!(renewals(&authority), 0);
    // Paul's test renews his own, then starts with it; Marc's left as is.
    let outcome = tested(&paul, &id).await;
    assert_eq!(outcome["ok"], json!(true), "{outcome}");
    assert_eq!(renewals(&authority), 1);
    assert_ne!(person_kept(&api, &id, user::PAUL).await, pauls);
    assert_eq!(person_kept(&api, &id, user::MARC).await, marcs);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_declaration_asks_nothing_and_an_unopenable_refresh_token_is_said() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "autresoi").await;
    let connectors = &api.app.services.connectors;
    let as_marc = as_person(user::MARC);

    // Given under another issuer than the one declared: never renewed there.
    authorizes(&marc, &authority, &id).await;
    let elsewhere = "issuer = 'https://autre-emetteur.example', \
                     access_expires_at = now() + interval '210 seconds'";
    for_person(&api, &id, user::MARC, elsewhere).await;
    let before = person_kept(&api, &id, user::MARC).await;
    assert!(!sees_for(connectors, "autresoi", as_marc).await);
    assert_eq!(renewals(&authority), 0);
    assert_eq!(person_kept(&api, &id, user::MARC).await, before);

    // The server now the company's to authorize, his left behind: never
    // renewed, nor used.
    authorizes(&marc, &authority, &id).await;
    let company = "update connector_instances set config = config #- '{oauth,holder}' \
                   where id = $1";
    alter(&api, &id, company).await;
    for_person(&api, &id, user::MARC, SOON).await;
    let before = person_kept(&api, &id, user::MARC).await;
    assert!(!sees_for(connectors, "autresoi", as_marc).await);
    assert_eq!(renewals(&authority), 0);
    assert_eq!(person_kept(&api, &id, user::MARC).await, before);

    // Each person's again, his refresh token as if sealed under another
    // master key: never sent, said in his account.
    let each = "update connector_instances set config = \
                jsonb_set(config, '{oauth,holder}', '\"person\"') where id = $1";
    alter(&api, &id, each).await;
    let spoilt = format!("refresh_ciphertext = '\\x00', {UNCLAIMED}, {EXPIRING}");
    for_person(&api, &id, user::MARC, &spoilt).await;
    assert!(!sees_for(connectors, "autresoi", as_marc).await);
    assert_eq!(renewals(&authority), 0);
    let shown = own_shown(&marc, &id).await;
    let failure = shown["renewalFailure"].as_str().unwrap_or_default();
    assert!(
        failure.contains("autorisez de nouveau ce serveur dans « Mon compte »"),
        "{shown}"
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
