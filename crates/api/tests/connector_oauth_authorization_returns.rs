//! The returns an MCP server's authorization refuses (ADR-171): without
//! the attempt's cookie, with another attempt's state, refused by the
//! authorization server, from another issuer or without the one it
//! announces, to another administrator (who may finish one:
//! `connector_oauth_authorization_rights.rs`), after the declaration
//! changed, or with tokens the platform could not keep; each sends the
//! browser back saying it failed, its reason audited without a code.
//! Starting needs a company-wide connector administrator and a declared
//! server.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer};
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::accounts::{activate, create_partial_admin};
use support::connector_oauth::{
    FAILED, api_path, authorization_entries, come_back, declared, kept_tokens, refusals_by,
    refused_because, start, try_start, with_param,
};
use support::mcp::PRIVATE_DEPLOYMENT;
use support::sso::location;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_return_that_is_not_the_attempt_s_own_is_refused() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "returns").await;

    // Without the attempt's cookie.
    let (url, cookie) = start(&paul, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    assert_eq!(location(&come_back(&paul, &back, None).await), FAILED);
    // With another attempt's state.
    let (url, cookie_b) = start(&paul, &id).await;
    let back_b = api_path(&authority.sent_back(&url).await);
    let other = come_back(&paul, &back, Some(&cookie_b)).await;
    assert_eq!(location(&other), FAILED);
    assert!(refused_because(&api, &id).await.contains("autre tentative"));
    // Refused by the authorization server: its error code, nothing else.
    let refused = with_param(
        &with_param(&back_b, "code", None),
        "error",
        Some("access_denied"),
    );
    assert_eq!(
        location(&come_back(&paul, &refused, Some(&cookie_b)).await),
        FAILED
    );
    assert!(refused_because(&api, &id).await.contains("(access_denied)"));
    // From another issuer than the one declared.
    let (url, cookie_c) = start(&paul, &id).await;
    let back_c = api_path(&authority.sent_back(&url).await);
    let forged = with_param(&back_c, "iss", Some("https://evil.example"));
    assert_eq!(
        location(&come_back(&paul, &forged, Some(&cookie_c)).await),
        FAILED
    );
    assert!(refused_because(&api, &id).await.contains("autre émetteur"));
    // To another administrator than the one who started it.
    let mut account = create_partial_admin(&api, &paul, "admin.equipe.autorisation").await;
    let partial = activate(&api, &mut account).await;
    let (url, cookie_d) = start(&paul, &id).await;
    let back_d = api_path(&authority.sent_back(&url).await);
    assert_eq!(
        location(&come_back(&partial, &back_d, Some(&cookie_d)).await),
        FAILED
    );
    assert_eq!(
        refusals_by(&api, account.id).await,
        ["Tentative d’un autre administrateur"]
    );
    // The declaration changed meanwhile (its scopes).
    let (url, cookie_e) = start(&paul, &id).await;
    let back_e = api_path(&authority.sent_back(&url).await);
    let again = json!({
        "clientId": CLIENT_ID,
        "clientSecret": CLIENT_SECRET,
        "scopes": ["files:read", "offline_access"],
    });
    let redeclared = paul
        .put(&format!("/admin/connectors/{id}/oauth"), again)
        .await;
    assert_eq!(redeclared.status, StatusCode::OK);
    assert_eq!(
        location(&come_back(&paul, &back_e, Some(&cookie_e)).await),
        FAILED
    );
    assert!(refused_because(&api, &id).await.contains("changée"));
    // Nothing kept, no code audited.
    assert!(kept_tokens(&api, &id).await.is_none());
    let entries = authorization_entries(&api, &id).await;
    let code = back.split("code=").nth(1).unwrap_or_default();
    let code = code.split('&').next().unwrap_or_default();
    assert!(!code.is_empty());
    assert!(entries.iter().all(|(_, _, reason, metadata)| {
        !reason.clone().unwrap_or_default().contains(code) && !metadata.to_string().contains(code)
    }));
    let _ = cookie;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_authorization_server_that_names_its_issuer_must_and_tokens_must_last() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    // It says its return names the issuer: one without is refused.
    authority.set(|settings| settings.iss_on_return = true);
    let (_, id) = declared(&paul, &authority, "issuer").await;
    let (url, cookie) = start(&paul, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    let without = with_param(&back, "iss", None);
    assert_eq!(
        location(&come_back(&paul, &without, Some(&cookie)).await),
        FAILED
    );
    assert!(refused_because(&api, &id).await.contains("sans l’émetteur"));
    // Tokens that expire without a refresh token: refused, nothing kept.
    authority.set(|settings| settings.refresh = false);
    let (url, cookie) = start(&paul, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    assert_eq!(
        location(&come_back(&paul, &back, Some(&cookie)).await),
        FAILED
    );
    assert!(refused_because(&api, &id).await.contains("renouvellement"));
    assert!(kept_tokens(&api, &id).await.is_none());
    // Signed out: to the sign-in page.
    let anonymous = api.anonymous();
    assert_eq!(
        location(&come_back(&anonymous, &back, Some(&cookie)).await),
        "/login"
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_company_wide_administrator_starts_one_on_a_declared_server() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "starts").await;
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.depart").await;
    let partial = activate(&api, &mut partial).await;
    assert_eq!(try_start(&partial, &id).await.status, StatusCode::FORBIDDEN);
    let authorization = format!("/admin/connectors/{id}/oauth/authorization");
    assert_eq!(
        partial.delete(&authorization).await.status,
        StatusCode::FORBIDDEN
    );
    let absent = "00000000-0000-0000-0000-000000000001";
    assert_eq!(try_start(&paul, absent).await.status, StatusCode::NOT_FOUND);
    // Not declared: nothing to start.
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}/oauth"))
            .await
            .status,
        StatusCode::OK
    );
    let undeclared = try_start(&paul, &id).await;
    assert_eq!(undeclared.status, StatusCode::BAD_REQUEST);
    assert!(
        undeclared.text().contains("Déclarez"),
        "{}",
        undeclared.text()
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
