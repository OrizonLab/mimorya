//! A declaration replaces only what its administrator saw (ADR-176): it
//! names the version of the server's OAuth its form was opened on, which
//! changes with what is declared, who authorizes the server, the
//! authorizations declaring again takes (one withdrawn and another given
//! included), and its client's secret written again. Naming one changed since, it is refused before anything is asked
//! of the server, or under the lock when the change came while the server
//! was being found; the change stays. Naming none, it is not checked.

mod support;

use std::sync::Arc;

use axum::http::StatusCode;
use mimorya_connectors::testing::{
    FakeAuthorizationServer, Hold, OAuthQuirk, serve_oauth_mcp_http,
};
use mimorya_core::demo::user;
use serde_json::json;
use support::connector_oauth::{AUTHORIZED, authorize, holding, kept_tokens};
use support::mcp::{PRIVATE_DEPLOYMENT, add, listed, mcp_body};
use support::person_authorizations::{
    PERSON_AUTHORIZED, declared_per_person, person_authorize, person_kept,
};
use support::person_identity::declare;
use support::sso::location;
use support::{Client, TestApi, TestResponse};

const CHANGED: &str = "a changé depuis l’ouverture de ce formulaire";

/// The version of the server's OAuth a form would be opened on.
async fn version(admin: &Client<'_>, id: &str) -> String {
    let listed = listed(admin, id).await;
    listed["mcp"]["oauthVersion"]
        .as_str()
        .expect("a version")
        .to_string()
}

/// Declared again with the test client, from a form opened on `opened`.
async fn replacing(admin: &Client<'_>, id: &str, opened: &str) -> TestResponse {
    declare(admin, id, json!({ "replaces": opened })).await
}

fn assert_changed(refused: &TestResponse) {
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(
        refused.error_message().contains(CHANGED),
        "{}",
        refused.text()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_declaration_opened_on_what_changed_since_is_refused() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let id = add(
        &paul,
        mcp_body("Fichiers versions", "versions", &server.url, None),
    )
    .await;

    // Two forms opened on no declaration: the first declares it, the
    // second is refused before anything is asked of the server.
    let none = version(&paul, &id).await;
    let first = replacing(&paul, &id, &none).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    let declared = version(&paul, &id).await;
    assert_ne!(declared, none);
    let asked = authority.metadata_asked();
    assert_changed(&replacing(&paul, &id, &none).await);
    assert_eq!(authority.metadata_asked(), asked);

    // Authorized by the company since: what declaring again would take.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let authorized = version(&paul, &id).await;
    assert_ne!(authorized, declared);
    assert_changed(&replacing(&paul, &id, &declared).await);
    assert!(kept_tokens(&api, &id).await.is_some());

    // For each person to authorize, and Marc does: each changes it.
    let holder = format!("/admin/connectors/{id}/oauth/holder");
    let each = paul.put(&holder, json!({ "perPerson": true })).await;
    assert_eq!(each.status, StatusCode::OK, "{}", each.text());
    let per_person = version(&paul, &id).await;
    assert_ne!(per_person, authorized);
    let his = person_authorize(&marc, &authority, &id).await;
    assert_eq!(location(&his), PERSON_AUTHORIZED);
    let with_his = version(&paul, &id).await;
    assert_ne!(with_his, per_person);
    for opened in [&authorized, &per_person] {
        assert_changed(&replacing(&paul, &id, opened).await);
    }
    assert!(person_kept(&api, &id, user::MARC).await.is_some());
    assert_eq!(listed(&paul, &id).await["mcp"]["oauth"]["perPerson"], true);

    // Opened on what is there: declared again, Marc's authorization going.
    let current = replacing(&paul, &id, &with_his).await;
    assert_eq!(current.status, StatusCode::OK, "{}", current.text());
    assert!(person_kept(&api, &id, user::MARC).await.is_none());

    // Declared again alike, naming no version (not checked): its client's
    // secret, written again, is all that changed, and a form opened before
    // is refused.
    let before = version(&paul, &id).await;
    let unchecked = declare(&paul, &id, json!({})).await;
    assert_eq!(unchecked.status, StatusCode::OK, "{}", unchecked.text());
    assert_changed(&replacing(&paul, &id, &before).await);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_change_made_while_the_server_is_found_is_seen_under_the_lock() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "pendant").await;
    let opened = version(&paul, &id).await;

    // Declared from a form opened on the current version, while Marc
    // authorizes the server for himself: nothing else tells it.
    let hold = Arc::new(Hold::default());
    authority.set(|settings| settings.metadata_hold = Some(hold.clone()));
    let declaring = replacing(&paul, &id, &opened);
    let his = async {
        holding(&hold, 1).await;
        authority.set(|settings| settings.metadata_hold = None);
        let his = person_authorize(&marc, &authority, &id).await;
        hold.release();
        his
    };
    let (declaring, his) = tokio::join!(declaring, his);
    assert_eq!(location(&his), PERSON_AUTHORIZED);
    assert_changed(&declaring);
    assert!(person_kept(&api, &id, user::MARC).await.is_some());
    assert_ne!(version(&paul, &id).await, opened);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_authorization_withdrawn_and_another_given_changes_it() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "qui").await;

    // A version longer than any: refused as such, not as changed.
    let long = replacing(&paul, &id, &"0".repeat(65)).await;
    assert_eq!(long.status, StatusCode::BAD_REQUEST, "{}", long.text());

    // Opened while Marc's is given; Marc withdraws it and Lucas gives his:
    // one person still, another, and the form opened before is refused.
    let his = person_authorize(&marc, &authority, &id).await;
    assert_eq!(location(&his), PERSON_AUTHORIZED);
    let opened = version(&paul, &id).await;
    let path = format!("/account/connector-authorizations/{id}");
    let withdrawn = marc.delete(&path).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    let theirs = person_authorize(&lucas, &authority, &id).await;
    assert_eq!(location(&theirs), PERSON_AUTHORIZED);
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    assert_eq!(oauth["personAuthorizations"], 1, "{oauth}");
    assert_changed(&replacing(&paul, &id, &opened).await);
    assert!(person_kept(&api, &id, user::LUCAS).await.is_some());
}
