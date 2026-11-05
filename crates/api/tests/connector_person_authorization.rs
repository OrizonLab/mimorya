//! A person authorizes, for themself, an MCP server each person authorizes
//! (ADR-174): from their account, the browser at the authorization server
//! and back at the address the company's return uses, then sent to their
//! account; the tokens kept for them alone, sealed under their name, never
//! the company's nor anyone else's; listed in their account, never another
//! person's; withdrawn by them, revoked at the authorization server, even
//! once the server is disabled or they lost the right to use connectors.
//! Their account disabled, every authorization they gave goes, revoked.
//! Another company's administrator neither starts nor withdraws one.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::{role, user};
use serde_json::json;
use support::TestApi;
use support::accounts::{INVITED_PASSWORD, accept, activate, assignment_of, create_account};
use support::companies;
use support::connector_oauth::kept_tokens;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{
    PERSON_AUTHORIZED, declared_per_person, entries_by, person_authorize, person_count,
    person_kept, person_revocations,
};
use support::sso::location;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_authorizes_a_server_for_themself_and_withdraws_it() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let lea = api.sign_in(user::LEA).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared_per_person(&paul, &authority, "poursoi").await;

    // Listed in Marc's account, not yet authorized.
    let mine = marc.get("/account/connector-authorizations").await.json();
    let listed_server = mine["servers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["connectorId"] == id)
        .expect("listed")
        .clone();
    assert_eq!(listed_server["available"], true);
    assert!(listed_server["authorization"].is_null());

    // Authorized by Marc, for Marc alone.
    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_AUTHORIZED
    );
    let token = person_kept(&api, &id, user::MARC)
        .await
        .expect("kept for Marc");
    assert!(authority.accepts(&token, &server.url));
    assert!(person_kept(&api, &id, user::LEA).await.is_none());
    assert!(
        kept_tokens(&api, &id).await.is_none(),
        "never the company's"
    );
    assert_eq!(
        listed(&paul, &id).await["mcp"]["oauth"]["personAuthorizations"],
        1
    );
    let audited = entries_by(
        &api,
        Some(&id),
        user::MARC,
        "connector.person_oauth_authorized",
    )
    .await;
    assert_eq!(audited, vec![("success".to_string(), None)]);
    let mine = marc.get("/account/connector-authorizations").await.json();
    let state = &mine["servers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["connectorId"] == id)
        .unwrap()["authorization"];
    assert!(state["authorizedAt"].is_string());
    assert!(!mine.to_string().contains(&token), "never a token");
    // Léa sees no authorization of hers, and cannot withdraw Marc's.
    let hers = lea.get("/account/connector-authorizations").await.json();
    assert!(hers.to_string().contains(&id));
    assert!(!hers.to_string().contains("authorizedAt"));
    let path = format!("/account/connector-authorizations/{id}");
    assert_eq!(lea.delete(&path).await.status, StatusCode::NOT_FOUND);
    assert!(person_kept(&api, &id, user::MARC).await.is_some());

    // Withdrawn by Marc: gone, revoked at the authorization server.
    assert_eq!(marc.delete(&path).await.status, StatusCode::OK);
    assert!(person_kept(&api, &id, user::MARC).await.is_none());
    assert!(!authority.accepts(&token, &server.url));
    let removed = entries_by(
        &api,
        Some(&id),
        user::MARC,
        "connector.person_oauth_removed",
    )
    .await;
    assert_eq!(removed.len(), 1);
    let revoked = person_revocations(&api, &id).await;
    assert_eq!(
        revoked.last().expect("audited").2,
        json!({ "revoked": 1, "failed": 0 })
    );
    assert_eq!(marc.delete(&path).await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_account_s_authorizations_are_revoked() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared_per_person(&paul, &authority, "depart").await;
    let (other_server, other) = declared_per_person(&paul, &authority, "departautre").await;
    let mut account = create_account(&paul, "autorise.depart").await;
    let granted = paul
        .post(
            &format!("/admin/users/{}/assignments", account.id),
            json!({ "roleId": role::MEMBER, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::CREATED, "{}", granted.text());
    let leaving = activate(&api, &mut account).await;
    for connector in [&id, &other] {
        assert_eq!(
            location(&person_authorize(&leaving, &authority, connector).await),
            PERSON_AUTHORIZED
        );
    }
    let token = person_kept(&api, &id, account.id).await.expect("kept");
    let other_token = person_kept(&api, &other, account.id).await.expect("kept");
    // Marc's stays whatever happens to theirs.
    let marc = api.sign_in(user::MARC).await;
    person_authorize(&marc, &authority, &id).await;
    let marc_token = person_kept(&api, &id, user::MARC).await.expect("kept");

    let disabled = paul
        .patch(
            &format!("/admin/users/{}", account.id),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert!(person_kept(&api, &id, account.id).await.is_none());
    assert!(person_kept(&api, &other, account.id).await.is_none());
    assert!(!authority.accepts(&token, &server.url));
    assert!(!authority.accepts(&other_token, &other_server.url));
    assert!(authority.accepts(&marc_token, &server.url));
    assert_eq!(person_count(&api, &id).await, 1);
    for connector in [&id, &other] {
        let revoked = person_revocations(&api, connector).await;
        assert_eq!(
            revoked.last().expect("audited").2,
            json!({ "revoked": 1, "failed": 0 })
        );
    }
    for connector in [id, other] {
        assert_eq!(
            paul.delete(&format!("/admin/connectors/{connector}"))
                .await
                .status,
            StatusCode::OK
        );
    }
}

/// The servers listed in `person`'s account, by id: whether they may
/// authorize it, and whether they did.
async fn listed_for(person: &support::Client<'_>) -> Vec<(String, bool, bool)> {
    let mine = person.get("/account/connector-authorizations").await.json();
    mine["servers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|server| {
            (
                server["connectorId"].as_str().unwrap().to_string(),
                server["available"].as_bool().unwrap(),
                server["authorization"]["authorizedAt"].is_string(),
            )
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_a_person_authorized_stays_theirs_to_withdraw() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared_per_person(&paul, &authority, "retirable").await;
    // Never listed: a server the company authorizes, one disabled that no
    // one authorized.
    let (_, company) = support::connector_oauth::declared(&paul, &authority, "entrep").await;
    let (_, off) = declared_per_person(&paul, &authority, "eteint").await;
    let instance = |id: &str| format!("/admin/connectors/{id}");
    let disabled = paul
        .patch(&instance(&off), json!({ "enabled": false }))
        .await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_AUTHORIZED
    );
    let listed = listed_for(&marc).await;
    assert!(listed.contains(&(id.clone(), true, true)), "{listed:?}");
    assert!(
        listed
            .iter()
            .all(|(listed, ..)| *listed != company && *listed != off)
    );

    // Disabled once authorized: listed, not available, still withdrawable.
    let token = person_kept(&api, &id, user::MARC).await.expect("kept");
    let disabled = paul
        .patch(&instance(&id), json!({ "enabled": false }))
        .await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert!(listed_for(&marc).await.contains(&(id.clone(), false, true)));
    let path = format!("/account/connector-authorizations/{id}");
    assert_eq!(marc.delete(&path).await.status, StatusCode::OK);
    assert!(!authority.accepts(&token, &server.url));

    // Their right to use connectors withdrawn: listed, withdrawable too.
    let enabled = paul.patch(&instance(&id), json!({ "enabled": true })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    let mut account = create_account(&paul, "droit.retire").await;
    let losing = activate(&api, &mut account).await;
    person_authorize(&losing, &authority, &id).await;
    let token = person_kept(&api, &id, account.id).await.expect("kept");
    let member = assignment_of(&paul, account.id, role::MEMBER).await;
    let withdrawn = paul.delete(&format!("/admin/assignments/{member}")).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    assert_eq!(listed_for(&losing).await, vec![(id.clone(), false, true)]);
    assert_eq!(losing.delete(&path).await.status, StatusCode::OK);
    assert!(!authority.accepts(&token, &server.url));

    // Another company's administrator neither starts nor withdraws one.
    let address = "jeanne.martin@acme-soi.example";
    let acme = companies::create(&api, "acme-soi", address).await;
    let accepted = accept(&api, &acme.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    let start = jeanne.post(&path, json!({})).await;
    assert_eq!(start.status, StatusCode::NOT_FOUND, "{}", start.text());
    assert_eq!(jeanne.delete(&path).await.status, StatusCode::NOT_FOUND);
    for connector in [id, company, off] {
        assert_eq!(
            paul.delete(&instance(&connector)).await.status,
            StatusCode::OK
        );
    }
}
