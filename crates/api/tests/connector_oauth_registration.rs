//! The platform registered as an MCP server's OAuth client by itself (RFC
//! 7591, ADR-173): an administrator declares OAuth asking for it, the
//! platform registers at the authorization server the discovery found, for
//! its own return address, keeps the granted secret sealed (never shown nor
//! audited) and the declaration says the client is its own; it is then
//! authorized like a typed one, and runs see the server's tools. A body
//! names its client one way only; a server that offers no registration or
//! asks for an initial token is said, nothing kept.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::mcp::oauth::{NO_REGISTRATION_ENDPOINT, REGISTRATION_TOKEN_NEEDED};
use mimorya_connectors::testing::{
    CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http,
};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::connector_oauth::{AUTHORIZED, authorize, kept_tokens};
use support::connector_sessions::runs_see;
use support::mcp::{PRIVATE_DEPLOYMENT, add, listed, mcp_body};
use support::sso::location;

/// The audit entries of this connector's declarations: their metadata.
async fn declarations(api: &TestApi, id: &str) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<Value> = sqlx::query_scalar(
        "select metadata from audit_log where resource_id = $1 \
         and action = 'connector.oauth_declared' order by id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_platform_registers_its_own_client_and_is_authorized_with_it() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let id = add(
        &paul,
        mcp_body("Fichiers inscrits", "inscrit", &server.url, None),
    )
    .await;
    let oauth = format!("/admin/connectors/{id}/oauth");
    let declared = paul.put(&oauth, json!({ "register": true })).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    // Registered at the server for the platform's return address alone.
    let registered = authority.registered();
    assert_eq!(registered.len(), 1);
    let requests = authority.requests();
    let asked = requests.iter().find(|r| r.endpoint == "register").unwrap();
    let back = paul.get("/admin/connectors").await.json()["oauthRedirectUri"].clone();
    assert_eq!(asked.params["redirect_uris"], back.as_str().unwrap());
    // Said as its own, its secret sealed and never shown nor audited.
    let shown = declared.json();
    assert_eq!(shown["registered"], true, "{shown}");
    assert_eq!(shown["clientId"], registered[0].as_str());
    assert_eq!(shown["clientAuth"], "client_secret_post");
    assert_eq!(shown["clientSecretStored"], true);
    assert!(shown["clientSecretExpiresAt"].is_null());
    assert_eq!(listed(&paul, &id).await["mcp"]["oauth"]["registered"], true);
    let audited = declarations(&api, &id).await;
    assert_eq!(audited.last().unwrap()["registered"], true);
    assert!(!format!("{audited:?}{shown}").contains("secret-client"));
    // Authorized with it; runs see the server's tools.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let (access, _) = kept_tokens(&api, &id).await.expect("kept");
    assert!(authority.accepts(&access, &server.url));
    assert!(runs_see(&api, "inscrit").await);
    // Declared again by hand: a typed client, no longer its own.
    let typed = json!({ "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET });
    let again = paul.put(&oauth, typed).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    assert_eq!(again.json()["registered"], false);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_client_is_named_one_way_only_and_a_server_that_cannot_register_keeps_nothing() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let closed = FakeAuthorizationServer::start().await;
    closed.set(|settings| settings.registration = false);
    let server = serve_oauth_mcp_http(&closed, OAuthQuirk::None).await;
    let id = add(
        &paul,
        mcp_body("Fichiers fermés", "ferme", &server.url, None),
    )
    .await;
    let oauth = format!("/admin/connectors/{id}/oauth");
    // Its own and a typed one at once, or none at all: refused as such.
    for (body, said) in [
        (
            json!({ "register": true, "clientId": CLIENT_ID }),
            "ne les donnez pas",
        ),
        (
            json!({ "register": true, "clientSecret": CLIENT_SECRET }),
            "ne les donnez pas",
        ),
        (json!({}), "Identifiant du client requis"),
        (json!({ "register": false }), "Identifiant du client requis"),
    ] {
        let refused = paul.put(&oauth, body.clone()).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}");
        assert!(refused.text().contains(said), "{body}: {}", refused.text());
    }
    // A server that offers no registration: said, nothing kept.
    let refused = paul.put(&oauth, json!({ "register": true })).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(
        refused.text().contains(NO_REGISTRATION_ENDPOINT),
        "{}",
        refused.text()
    );
    assert!(listed(&paul, &id).await["mcp"]["oauth"].is_null());
    // One that asks for an initial access token: the same.
    let guarded = FakeAuthorizationServer::start().await;
    guarded.set(|settings| settings.registration_token = true);
    let elsewhere = serve_oauth_mcp_http(&guarded, OAuthQuirk::None).await;
    let other = add(
        &paul,
        mcp_body("Fichiers gardés", "garde", &elsewhere.url, None),
    )
    .await;
    let refused = paul
        .put(
            &format!("/admin/connectors/{other}/oauth"),
            json!({ "register": true }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(
        refused.text().contains(REGISTRATION_TOKEN_NEEDED),
        "{}",
        refused.text()
    );
    assert!(listed(&paul, &other).await["mcp"]["oauth"].is_null());
    for connector in [&id, &other] {
        let deleted = paul.delete(&format!("/admin/connectors/{connector}")).await;
        assert_eq!(deleted.status, StatusCode::OK);
    }
}
