//! An account the company's identity provider disables through SCIM
//! (ADR-183) loses what a disabling by the administration takes (ADR-174):
//! its own authorizations of MCP servers taken and revoked at their
//! authorization server, audited as let go by the platform. Another
//! person's stay. And a token of another company changes nothing of this
//! one's accounts.

mod support;

use axum::http::{Method, StatusCode};
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::{role, user};
use serde_json::json;
use support::accounts::{INVITED_PASSWORD, accept, activate, create_account, directory_entry};
use support::mcp::PRIVATE_DEPLOYMENT;
use support::person_authorizations::{
    PERSON_AUTHORIZED, declared_per_person, person_authorize, person_kept, person_revocations,
};
use support::scim::{made, patch_of, refusal, send};
use support::sso::location;
use support::{TestApi, companies};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_the_provider_disables_loses_its_server_authorizations() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared_per_person(&paul, &authority, "scimdepart").await;
    let mut account = create_account(&paul, "scim.depart").await;
    let granted = paul
        .post(
            &format!("/admin/users/{}/assignments", account.id),
            json!({ "roleId": role::MEMBER, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::CREATED, "{}", granted.text());
    let leaving = activate(&api, &mut account).await;
    let authorized = person_authorize(&leaving, &authority, &id).await;
    assert_eq!(location(&authorized), PERSON_AUTHORIZED);
    let token = person_kept(&api, &id, account.id).await.expect("kept");
    let marc = api.sign_in(user::MARC).await;
    person_authorize(&marc, &authority, &id).await;
    let marc_token = person_kept(&api, &id, user::MARC).await.expect("kept");

    let (_, secret) = made(&paul, "Microsoft Entra ID").await;
    let off = patch_of(json!([{ "op": "Replace", "path": "active", "value": "False" }]));
    let path = format!("/Users/{}", account.id);
    let disabled = send(&api, Some(&secret), Method::PATCH, &path, &off).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());

    // Theirs taken and revoked, Marc's kept.
    assert!(person_kept(&api, &id, account.id).await.is_none());
    assert!(!authority.accepts(&token, &server.url));
    assert!(authority.accepts(&marc_token, &server.url));
    let revoked = person_revocations(&api, &id).await;
    assert_eq!(
        revoked.last().expect("audited").2,
        json!({ "revoked": 1, "failed": 0 })
    );
    let mut tx = api
        .app
        .services
        .db
        .begin(mimorya_core::demo::tenant::ID)
        .await
        .unwrap();
    let actors: Vec<(String, Option<uuid::Uuid>)> = sqlx::query_as(
        "select actor_type, actor_user_id from audit_log where resource_id = $1 \
         and action = 'connector.oauth_person_authorizations_revoked' order by id",
    )
    .bind(&id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(actors.last(), Some(&("system".to_string(), None)));
    let deleted = paul.delete(&format!("/admin/connectors/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_token_changes_nothing_here() {
    let api = TestApi::start().await;
    let address = "jeanne.martin@acme-change.example";
    let company = companies::create(&api, "acme-change", address).await;
    let accepted = accept(&api, &company.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    let (_, theirs) = made(&jeanne, "Okta Acme").await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = format!("/Users/{}", user::MARC);
    for (method, body) in [
        (
            Method::PATCH,
            patch_of(json!([{ "op": "replace", "path": "active", "value": false }])),
        ),
        (
            Method::PUT,
            json!({ "userName": "marc.lefevre@neo-maintenance.example", "active": false }),
        ),
        (Method::DELETE, json!(null)),
    ] {
        let refused = send(&api, Some(&theirs), method.clone(), &marc, &body).await;
        assert_eq!(
            refusal(&refused),
            (StatusCode::NOT_FOUND, json!(null)),
            "{method}"
        );
    }
    let entry = directory_entry(&paul, user::MARC).await;
    assert_eq!(entry["status"], "active");
}
