//! A declaration finds its server outside any lock, which may take its
//! time (ADR-175): another administrator's change of the verification of
//! identities, or of who authorizes the server, made meanwhile, is never
//! undone unseen. The declaration that read the former state is refused,
//! to start again, and the change made meanwhile stays. A return keeps its
//! place among the company's administration starts (ADR-055) until it has
//! verified who authorized, which may take its time too.

mod support;

use std::sync::Arc;

use axum::http::StatusCode;
use mimorya_api::services::connectors::{MAX_ADMIN_STARTS, TOO_MANY_ADMIN_STARTS};
use mimorya_connectors::testing::{FakeAuthorizationServer, Hold, serve_demo_docs_http};
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::connector_oauth::holding;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{PERSON_AUTHORIZED, declared_per_person, person_authorize};
use support::person_identity::{PERSON_OTHER_ACCOUNT, declare, declared_verifying, signed_in_as};
use support::sso::location;

/// The authorization server's metadata held from now on: the declaration
/// asked next finds the server only once the hold is released.
fn holding_metadata(authority: &FakeAuthorizationServer) -> Arc<Hold> {
    let hold = Arc::new(Hold::default());
    authority.set(|settings| settings.metadata_hold = Some(hold.clone()));
    hold
}

/// Once a declaration is held finding the server: the requests that
/// follow are not.
async fn finding(authority: &FakeAuthorizationServer, hold: &Hold) {
    holding(hold, 1).await;
    authority.set(|settings| settings.metadata_hold = None);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_change_made_while_a_declaration_finds_its_server_is_never_undone() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    signed_in_as(&authority, user::PAUL);
    let (_, id) = declared_per_person(&paul, &authority, "pendant").await;

    // Declared again without a word of it, while another turns it on.
    let hold = holding_metadata(&authority);
    let unsaid = declare(&paul, &id, json!({}));
    let turned_on = async {
        finding(&authority, &hold).await;
        let turned_on = declare(&paul, &id, json!({ "verifyIdentity": true })).await;
        hold.release();
        turned_on
    };
    let (unsaid, turned_on) = tokio::join!(unsaid, turned_on);
    assert_eq!(turned_on.status, StatusCode::OK, "{}", turned_on.text());
    assert_eq!(unsaid.status, StatusCode::CONFLICT, "{}", unsaid.text());
    assert!(unsaid.text().contains("recommencez"), "{}", unsaid.text());
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    assert_eq!(oauth["verifiesIdentity"], true, "{oauth}");

    // Asked for, while another makes the company authorize the server.
    let hold = holding_metadata(&authority);
    let asked = declare(&paul, &id, json!({ "verifyIdentity": true }));
    let to_company = async {
        finding(&authority, &hold).await;
        let path = format!("/admin/connectors/{id}/oauth/holder");
        let to_company = paul.put(&path, json!({ "perPerson": false })).await;
        hold.release();
        to_company
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
    let hold = Arc::new(Hold::default());
    authority.set(|settings| settings.configuration_hold = Some(hold.clone()));
    let his = person_authorize(&marc, &authority, &id);
    let other = person_authorize(&lucas, &authority, &id);
    let third = async {
        holding(&hold, 2).await;
        let refused = trial().await;
        hold.release();
        refused
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
