//! An administrator's test of a server each person authorizes (ADR-174): with
//! their own authorization, never another person's nor the company's; refused
//! without it, or with one given under another declaration; how it went is
//! theirs, not written on the server's status, an expired or refused one
//! said to them to authorize again in their account; unreadable policies,
//! the server's own, written all the same. A person who authorized it does
//! not test it, nor does another company's administrator, who neither
//! changes who authorizes it.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::admin::TEST_OWN_AUTHORIZATION;
use mimorya_connectors::mcp::{OAUTH_EXPIRED_PERSON, OAUTH_REFUSED_PERSON};
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{INVITED_PASSWORD, accept};
use support::companies;
use support::connector_sessions::{alter, said, tested};
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{
    declared_per_person, last_metadata, person_authorized, person_authorized_until,
};

/// Whether the server was ever sent `token`.
fn sent(server: &mimorya_connectors::testing::ProtectedMcp, token: &str) -> bool {
    let bearer = format!("Bearer {token}");
    server
        .authorizations()
        .iter()
        .any(|header| header.as_deref() == Some(bearer.as_str()))
}

/// The server's status as listed, and why it last failed.
async fn status(admin: &support::Client<'_>, id: &str) -> (Value, Value) {
    let connector = listed(admin, id).await;
    (connector["status"].clone(), connector["lastError"].clone())
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_tests_with_their_own_authorization_alone() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared_per_person(&paul, &authority, "testsoi").await;
    let path = format!("/admin/connectors/{id}/test");
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    let resource = oauth["resource"].as_str().unwrap().to_string();
    let before = status(&paul, &id).await;

    // Marc authorized it; Paul has not: refused, Marc's never sent.
    let marcs = person_authorized(&api, &id, user::MARC, &authority, &resource).await;
    let refused = paul.post(&path, json!({})).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(
        refused
            .text()
            .contains("autorisez-le d’abord dans « Mon compte »")
    );
    // Marc, who authorized it, does not administer connectors.
    let not_his = marc.post(&path, json!({})).await;
    assert_eq!(not_his.status, StatusCode::FORBIDDEN, "{}", not_his.text());

    // Paul's given under another declaration: refused, never sent.
    let elsewhere = "https://ailleurs.example/mcp";
    let stale = person_authorized(&api, &id, user::PAUL, &authority, elsewhere).await;
    let refused = paul.post(&path, json!({})).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(refused.text().contains(TEST_OWN_AUTHORIZATION));

    // His own: tested with it, his name in the audit, the status untouched.
    let pauls = person_authorized(&api, &id, user::PAUL, &authority, &resource).await;
    let outcome = tested(&paul, &id).await;
    assert_eq!(outcome["error"], Value::Null, "{outcome}");
    let tools = outcome["tools"].as_array().unwrap().len();
    assert!(tools > 0, "{outcome}");
    assert!(sent(&server, &pauls));
    assert!(!sent(&server, &marcs) && !sent(&server, &stale));
    assert_eq!(
        last_metadata(&api, &id, "connector.tested").await,
        json!({ "ok": true, "tools": tools, "ownAuthorization": true })
    );
    assert_eq!(status(&paul, &id).await, before);

    // His expired: said to him, to authorize it again in his account, not
    // sent, not written on the server.
    let past = Some(chrono::Utc::now() - chrono::Duration::minutes(5));
    let expired = person_authorized_until(&api, &id, user::PAUL, &authority, &resource, past).await;
    let outcome = tested(&paul, &id).await;
    assert!(said(&outcome).contains(OAUTH_EXPIRED_PERSON), "{outcome}");
    assert!(!sent(&server, &expired));
    assert_eq!(status(&paul, &id).await, before);

    // His refused by the server: said to him alike.
    let pauls = person_authorized(&api, &id, user::PAUL, &authority, &resource).await;
    authority.revoke_all();
    let outcome = tested(&paul, &id).await;
    assert!(said(&outcome).contains(OAUTH_REFUSED_PERSON), "{outcome}");
    assert!(!said(&outcome).contains(&pauls));
    assert_eq!(
        last_metadata(&api, &id, "connector.tested").await,
        json!({ "ok": false, "tools": 0, "ownAuthorization": true })
    );
    assert_eq!(status(&paul, &id).await, before);
    assert!(!sent(&server, &marcs));

    // Its policies unreadable: the server's own, written for everyone.
    let spoilt = "update connector_instances set tool_policies = \
                  '{\"outil\": {\"risk\": \"inconnu\"}}' where id = $1";
    alter(&api, &id, spoilt).await;
    let outcome = tested(&paul, &id).await;
    assert!(
        said(&outcome).contains("Politiques d’outils illisibles"),
        "{outcome}"
    );
    let (state, error) = status(&paul, &id).await;
    assert_eq!(state, json!("error"));
    assert!(error.as_str().unwrap_or_default().contains("illisibles"));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_administrator_neither_tests_nor_changes_who_authorizes() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared_per_person(&paul, &authority, "testautre").await;
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    let resource = oauth["resource"].as_str().unwrap().to_string();
    let pauls = person_authorized(&api, &id, user::PAUL, &authority, &resource).await;

    let address = "jeanne.martin@acme-test.example";
    let acme = companies::create(&api, "acme-test", address).await;
    let accepted = accept(&api, &acme.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    let test = jeanne
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await;
    assert_eq!(test.status, StatusCode::NOT_FOUND, "{}", test.text());
    let holder = format!("/admin/connectors/{id}/oauth/holder");
    let changed = jeanne.put(&holder, json!({ "perPerson": false })).await;
    assert_eq!(changed.status, StatusCode::NOT_FOUND, "{}", changed.text());

    // Nothing sent, nothing changed: Paul's still his, still each person's.
    assert!(!sent(&server, &pauls));
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    assert_eq!(
        (
            oauth["perPerson"].clone(),
            oauth["personAuthorizations"].clone()
        ),
        (json!(true), json!(1))
    );
    assert!(authority.accepts(&pauls, &resource));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
