//! A declaration finds its server outside any lock, which may take its
//! time (ADR-175): another administrator's change of the verification of
//! identities, or of who authorizes the server, made meanwhile, is never
//! undone unseen. The declaration that read the former state is refused,
//! to start again, and the change made meanwhile stays. A return keeps its
//! place among the company's administration starts (ADR-055) until it has
//! verified who authorized, which may take its time too.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_api::services::connectors::{MAX_ADMIN_STARTS, TOO_MANY_ADMIN_STARTS};
use mimorya_connectors::testing::{FakeAuthorizationServer, serve_demo_docs_http};
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::connector_oauth::until_asked;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{PERSON_AUTHORIZED, declared_per_person, person_authorize};
use support::person_identity::{PERSON_OTHER_ACCOUNT, declare, declared_verifying, signed_in_as};
use support::sso::location;

/// Until a declaration asked for the authorization server's metadata
/// after `before` requests: it is finding the server.
async fn finding(authority: &FakeAuthorizationServer, before: usize) {
    until_asked(authority, FakeAuthorizationServer::metadata_asked, before).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_change_made_while_a_declaration_finds_its_server_is_never_undone() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    signed_in_as(&authority, user::PAUL);
    let (_, id) = declared_per_person(&paul, &authority, "pendant").await;
    let held = Some(Duration::from_secs(3));

    // Declared again without a word of it, while another turns it on.
    authority.set(|settings| settings.held_metadata = held);
    let before = authority.metadata_asked();
    let unsaid = declare(&paul, &id, json!({}));
    let turned_on = async {
        finding(&authority, before).await;
        authority.set(|settings| settings.held_metadata = None);
        declare(&paul, &id, json!({ "verifyIdentity": true })).await
    };
    let (unsaid, turned_on) = tokio::join!(unsaid, turned_on);
    assert_eq!(turned_on.status, StatusCode::OK, "{}", turned_on.text());
    assert_eq!(unsaid.status, StatusCode::CONFLICT, "{}", unsaid.text());
    assert!(unsaid.text().contains("recommencez"), "{}", unsaid.text());
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    assert_eq!(oauth["verifiesIdentity"], true, "{oauth}");

    // Asked for, while another makes the company authorize the server.
    authority.set(|settings| settings.held_metadata = held);
    let before = authority.metadata_asked();
    let asked = declare(&paul, &id, json!({ "verifyIdentity": true }));
    let to_company = async {
        finding(&authority, before).await;
        authority.set(|settings| settings.held_metadata = None);
        let path = format!("/admin/connectors/{id}/oauth/holder");
        paul.put(&path, json!({ "perPerson": false })).await
    };
    let (asked, to_company) = tokio::join!(asked, to_company);
    assert_eq!(to_company.status, StatusCode::OK, "{}", to_company.text());
    assert_eq!(asked.status, StatusCode::CONFLICT, "{}", asked.text());
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    assert_eq!(oauth["perPerson"], false, "{oauth}");
    assert_eq!(oauth["verifiesIdentity"], false, "{oauth}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_return_keeps_its_start_while_it_verifies_who_authorized() {
    assert_eq!(MAX_ADMIN_STARTS, 2);
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let authority = FakeAuthorizationServer::start().await;
    signed_in_as(&authority, user::MARC);
    let (_, id) = declared_verifying(&paul, &authority, "places").await;
    let answering = serve_demo_docs_http(None).await;
    let trial = || {
        let body = json!({ "prefix": "docs", "url": answering });
        paul.post("/admin/connectors/trial", body)
    };

    // Marc's return and Lucas's, signed in there as Marc, both verifying
    // who authorized while the server's OpenID configuration is held: a
    // third start, a trial, is refused meanwhile.
    authority.set(|settings| settings.held_configuration = Some(Duration::from_secs(4)));
    let before = authority.configuration_asked();
    let his = person_authorize(&marc, &authority, &id);
    let other = person_authorize(&lucas, &authority, &id);
    let third = async {
        let asked = FakeAuthorizationServer::configuration_asked;
        until_asked(&authority, asked, before + 1).await;
        authority.set(|settings| settings.held_configuration = None);
        trial().await
    };
    let (his, other, refused) = tokio::join!(his, other, third);
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert_eq!(refused.error_message(), TOO_MANY_ADMIN_STARTS);
    assert_eq!(location(&his), PERSON_AUTHORIZED);
    assert_eq!(location(&other), PERSON_OTHER_ACCOUNT);

    // Both verified: their places are free again.
    let accepted = trial().await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
}
