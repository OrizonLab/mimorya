//! An MCP server authorized by a company-wide connector administrator
//! (ADR-171): the browser sent to the authorization server the declaration
//! names, with PKCE for the resource, comes back in the administrator's
//! session; the tokens are sealed for the connector, never shown nor
//! audited; an authorization is replaced, withdrawn, and goes with the
//! declaration, the server's address or the connector.

mod support;

use std::collections::HashMap;

use axum::http::StatusCode;
use mimorya_connectors::testing::{
    CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, serve_demo_docs_http,
};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::connector_oauth::{
    AUTHORIZED, COOKIE, authorization_entries, authorize, declared, kept_tokens, start,
};
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::sso::{location, set_cookie};
use support::{TestApi, WEB_ORIGIN};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_authorizes_a_server_declared_with_oauth() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "files").await;

    // The departure: a code asked with PKCE, for the server, back here.
    let (url, _) = start(&paul, &id).await;
    let query: HashMap<String, String> = url::Url::parse(&url)
        .expect("url")
        .query_pairs()
        .into_owned()
        .collect();
    assert!(url.starts_with(&format!("{}/authorize?", authority.issuer)));
    assert_eq!(query["response_type"], "code");
    assert_eq!(query["client_id"], CLIENT_ID);
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["resource"], server.url);
    assert_eq!(
        query["redirect_uri"],
        format!("{WEB_ORIGIN}/api/admin/connectors/oauth/callback")
    );
    assert!(query["state"].len() >= 16 && query["code_challenge"].len() >= 43);

    // There and back: authorized, the attempt cleared.
    let back = authorize(&paul, &authority, &id).await;
    assert_eq!(location(&back), AUTHORIZED);
    let cleared = set_cookie(&back, COOKIE).expect("cleared");
    assert!(cleared.contains("Max-Age=0"));
    let (access, refresh) = kept_tokens(&api, &id).await.expect("kept");
    assert!(authority.accepts(&access, &server.url));
    assert!(refresh.is_some());
    let shown = listed(&paul, &id).await["mcp"]["oauth"]["authorization"].clone();
    assert_eq!(shown["renewable"], true);
    assert_eq!(shown["scopes"], json!(["files:read"]));
    assert!(shown["expiresAt"].is_string() && shown["authorizedAt"].is_string());
    // Never shown, never audited.
    assert!(!paul.get("/admin/connectors").await.text().contains(&access));
    let entries = authorization_entries(&api, &id).await;
    assert!(
        entries
            .iter()
            .all(|(_, _, _, m)| !m.to_string().contains(&access))
    );
    let (action, outcome, _, metadata) = entries.last().expect("audited");
    assert_eq!(
        (action.as_str(), outcome.as_str()),
        ("connector.oauth_authorized", "success")
    );
    assert_eq!(metadata["renewable"], true);
    assert_eq!(metadata["replaced"], false);

    // Again: replaced.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let (again, _) = kept_tokens(&api, &id).await.expect("kept");
    assert_ne!(again, access);
    let entries = authorization_entries(&api, &id).await;
    assert_eq!(entries.last().expect("audited").3["replaced"], true);

    // Withdrawn, once.
    let authorization = format!("/admin/connectors/{id}/oauth/authorization");
    assert_eq!(paul.delete(&authorization).await.status, StatusCode::OK);
    assert!(kept_tokens(&api, &id).await.is_none());
    assert_eq!(
        listed(&paul, &id).await["mcp"]["oauth"]["authorization"],
        Value::Null
    );
    assert_eq!(
        paul.delete(&authorization).await.status,
        StatusCode::BAD_REQUEST
    );
    let entries = authorization_entries(&api, &id).await;
    assert_eq!(
        entries.last().expect("audited").0,
        "connector.oauth_authorization_removed"
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_authorization_goes_with_its_declaration_its_address_and_its_connector() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "goes").await;
    let (instance, oauth) = (
        format!("/admin/connectors/{id}"),
        format!("/admin/connectors/{id}/oauth"),
    );
    // Declared again: the authorization does not follow.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let again = json!({ "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET });
    assert_eq!(paul.put(&oauth, again.clone()).await.status, StatusCode::OK);
    assert!(kept_tokens(&api, &id).await.is_none());
    // Withdrawn with the declaration.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    assert_eq!(paul.delete(&oauth).await.status, StatusCode::OK);
    assert!(kept_tokens(&api, &id).await.is_none());
    // Another address: neither.
    assert_eq!(paul.put(&oauth, again).await.status, StatusCode::OK);
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let elsewhere = serve_demo_docs_http(None).await;
    assert_eq!(
        paul.patch(&instance, json!({ "url": elsewhere }))
            .await
            .status,
        StatusCode::OK
    );
    assert!(kept_tokens(&api, &id).await.is_none());
    // The connector deleted: its authorization with it.
    assert_eq!(
        paul.patch(&instance, json!({ "url": server.url }))
            .await
            .status,
        StatusCode::OK
    );
    let declared = json!({ "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET });
    assert_eq!(paul.put(&oauth, declared).await.status, StatusCode::OK);
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    assert!(kept_tokens(&api, &id).await.is_some());
    assert_eq!(paul.delete(&instance).await.status, StatusCode::OK);
    assert!(kept_tokens(&api, &id).await.is_none());
}
