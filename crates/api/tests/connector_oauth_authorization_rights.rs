//! Who finishes an MCP server's authorization (ADR-171): only the
//! administrator who started it, while they still administer the company's
//! connectors, with the server's code, on a connector still there. Another
//! company-wide administrator, or another company's, presenting the
//! attempt's cookie and return, a starter whose role was withdrawn
//! meanwhile, a return without a code or for a connector deleted meanwhile:
//! refused, nothing kept, the reason audited.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::{role, user};
use serde_json::json;
use support::TestApi;
use support::accounts::{INVITED_PASSWORD, accept, activate, assignment_of, create_account};
use support::companies;
use support::connector_oauth::{
    AUTHORIZED, FAILED, api_path, come_back, declared, kept_tokens, refusals_by, refusals_in,
    refused_because, start, with_param,
};
use support::mcp::PRIVATE_DEPLOYMENT;
use support::sso::location;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_administrator_who_started_it_finishes_it_while_they_may() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "droits").await;
    // A second administrator of the whole company.
    let mut account = create_account(&paul, "admin.second.autorisation").await;
    let granted = paul
        .post(
            &format!("/admin/users/{}/assignments", account.id),
            json!({ "roleId": role::TENANT_ADMIN, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::CREATED, "{}", granted.text());
    let second = activate(&api, &mut account).await;

    // Paul's attempt and its return, presented by the second: refused,
    // nothing kept.
    let (url, cookie) = start(&paul, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    assert_eq!(
        location(&come_back(&second, &back, Some(&cookie)).await),
        FAILED
    );
    assert!(kept_tokens(&api, &id).await.is_none());
    assert_eq!(
        refusals_by(&api, account.id).await,
        ["Tentative d’un autre administrateur"]
    );
    // Still Paul's own: his return authorizes.
    assert_eq!(
        location(&come_back(&paul, &back, Some(&cookie)).await),
        AUTHORIZED
    );
    let (pauls, _) = kept_tokens(&api, &id).await.expect("kept");

    // The second starts, loses the role, comes back: refused, Paul's
    // authorization kept as it was.
    let (url, cookie) = start(&second, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    let assignment = assignment_of(&paul, account.id, role::TENANT_ADMIN).await;
    let revoked = paul
        .delete(&format!("/admin/assignments/{assignment}"))
        .await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
    assert_eq!(
        location(&come_back(&second, &back, Some(&cookie)).await),
        FAILED
    );
    assert!(refused_because(&api, &id).await.contains("retirés"));
    assert_eq!(kept_tokens(&api, &id).await.expect("kept").0, pauls);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_return_without_a_code_or_for_a_deleted_connector_is_refused() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "supprime").await;
    // Without the code, its state given back.
    let (url, cookie) = start(&paul, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    let without = with_param(&back, "code", None);
    assert_eq!(
        location(&come_back(&paul, &without, Some(&cookie)).await),
        FAILED
    );
    assert!(refused_because(&api, &id).await.contains("sans code"));
    // The connector deleted between the start and the return.
    let (url, cookie) = start(&paul, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
    assert_eq!(
        location(&come_back(&paul, &back, Some(&cookie)).await),
        FAILED
    );
    assert!(refused_because(&api, &id).await.contains("supprimé"));
    assert!(kept_tokens(&api, &id).await.is_none());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_administrator_does_not_finish_one() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "ailleurs").await;
    // Jeanne administers another company.
    let address = "jeanne.martin@acme-oauth.example";
    let acme = companies::create(&api, "acme-oauth", address).await;
    let accepted = accept(&api, &acme.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    // Paul's attempt and return, presented by her: refused in her company,
    // nothing kept in his.
    let (url, cookie) = start(&paul, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    assert_eq!(
        location(&come_back(&jeanne, &back, Some(&cookie)).await),
        FAILED
    );
    assert!(kept_tokens(&api, &id).await.is_none());
    assert_eq!(
        refusals_in(&api, acme.tenant_id, acme.admin_id).await,
        ["Tentative d’un autre administrateur"]
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
