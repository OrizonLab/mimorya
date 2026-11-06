//! A declaration finds its server outside any lock, which may take its
//! time (ADR-175): another administrator's change of the verification of
//! identities, or of who authorizes the server, made meanwhile, is never
//! undone unseen. The declaration that read the former state is refused,
//! to start again, and the change made meanwhile stays.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::declared_per_person;
use support::person_identity::{declare, signed_in_as};

/// Until the authorization server's metadata was asked once more than
/// `before`: a declaration is finding it. Failing, rather than waiting
/// forever, if none ever does.
async fn finding(authority: &FakeAuthorizationServer, before: usize) {
    let asked = tokio::time::timeout(Duration::from_secs(30), async {
        while authority.metadata_asked() <= before {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(asked.is_ok(), "no declaration asked for the metadata");
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
