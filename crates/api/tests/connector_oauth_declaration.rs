//! OAuth declared on an MCP server by a company-wide connector
//! administrator (ADR-171): found by the discovery, without the typed
//! token, which it replaces and which no longer goes with it; the client's
//! secret sealed, never shown nor audited; declared again, everything is
//! replaced; withdrawn, and gone with the server's address or the
//! connector. Until authorized, the connector starts no session.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{
    CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, OAuthQuirk, serve_demo_docs_http,
    serve_oauth_mcp_http,
};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_partial_admin};
use support::mcp::{PRIVATE_DEPLOYMENT, TOKEN, add, capture_logs, last_update, listed, mcp_body};

/// The audit entries of the connector `id`: action and metadata.
async fn audited(api: &TestApi, id: &str) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let entries =
        sqlx::query_as("select action, metadata from audit_log where resource_id = $1 order by id")
            .bind(id)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    entries
}

/// The sealed client secret of the connector `id`, if any.
async fn sealed_secret(api: &TestApi, id: &str) -> Option<Vec<u8>> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let sealed = sqlx::query_scalar("select ciphertext from secrets where name = $1")
        .bind(format!("connector.{id}.client_secret"))
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    sealed
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_declares_oauth_where_the_discovery_finds_it() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let id = add(
        &paul,
        mcp_body("Fichiers", "files", &server.url, Some(TOKEN)),
    )
    .await;
    let oauth = format!("/admin/connectors/{id}/oauth");

    let logs = capture_logs();
    let declared = paul
        .put(
            &oauth,
            json!({ "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET }),
        )
        .await;
    let logged = logs.text();
    drop(logs);
    assert!(!logged.contains(CLIENT_SECRET), "{logged}");
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    let declared = declared.json();
    assert_eq!(declared["issuer"], authority.issuer);
    assert_eq!(declared["resource"], server.url);
    assert_eq!(
        declared["tokenEndpoint"],
        format!("{}/token", authority.issuer)
    );
    assert_eq!(declared["clientAuth"], "client_secret_post");
    assert_eq!(declared["scopes"], json!(["files:read"]));
    assert_eq!(declared["clientSecretStored"], true);
    // The server was asked without any token, the typed one included.
    let asked = server.authorizations();
    assert!(!asked.is_empty() && asked.iter().all(Option::is_none));

    // Listed, the typed token gone, the secret never shown.
    let instance = listed(&paul, &id).await;
    assert_eq!(instance["mcp"]["tokenStored"], false);
    assert_eq!(instance["mcp"]["oauth"]["clientId"], CLIENT_ID);
    assert_eq!(instance["mcp"]["oauth"]["clientSecretStored"], true);
    assert!(
        !paul
            .get("/admin/connectors")
            .await
            .text()
            .contains(CLIENT_SECRET)
    );
    let sealed = sealed_secret(&api, &id).await.expect("sealed");
    assert!(
        !sealed
            .windows(CLIENT_SECRET.len())
            .any(|w| w == CLIENT_SECRET.as_bytes())
    );
    let entries = audited(&api, &id).await;
    assert!(
        entries
            .iter()
            .all(|(_, m)| !m.to_string().contains(CLIENT_SECRET))
    );
    let (_, metadata) = entries
        .iter()
        .find(|(action, _)| action == "connector.oauth_declared")
        .expect("audited");
    assert_eq!(metadata["tokenRemoved"], true);
    assert_eq!(metadata["clientSecretStored"], true);
    assert_eq!(metadata["replaced"], false);

    // Not authorized yet: no session, and said so.
    let tested = paul
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await
        .json();
    assert_eq!(tested["ok"], false, "{tested}");
    assert!(
        tested["error"]
            .as_str()
            .is_some_and(|e| e.contains("Autorisation OAuth à donner")),
        "{tested}"
    );

    // Declared again: a public client, its own scopes, the secret gone.
    let again = paul
        .put(
            &oauth,
            json!({
                "issuer": authority.issuer,
                "clientId": "client-public",
                "scopes": ["files:read", "offline_access"],
            }),
        )
        .await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    let again = again.json();
    assert_eq!(again["clientAuth"], "none");
    assert_eq!(again["clientSecretStored"], false);
    assert_eq!(again["scopes"], json!(["files:read", "offline_access"]));
    assert_eq!(sealed_secret(&api, &id).await, None);
    let replaced = audited(&api, &id).await;
    let (_, last) = replaced.last().expect("audited");
    assert_eq!(last["replaced"], true);

    // Withdrawn, once.
    assert_eq!(paul.delete(&oauth).await.status, StatusCode::OK);
    assert_eq!(listed(&paul, &id).await["mcp"]["oauth"], Value::Null);
    assert_eq!(paul.delete(&oauth).await.status, StatusCode::BAD_REQUEST);
    let entries = audited(&api, &id).await;
    assert_eq!(
        entries.last().expect("audited").0,
        "connector.oauth_removed"
    );
    let removed = paul.delete(&format!("/admin/connectors/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_oauth_declaration_follows_its_rules() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let id = add(&paul, mcp_body("Dossiers", "folders", &server.url, None)).await;
    let (instance, oauth) = (
        format!("/admin/connectors/{id}"),
        format!("/admin/connectors/{id}/oauth"),
    );

    // What the administrator writes is checked before the server is asked.
    for body in [
        json!({ "clientId": "" }),
        json!({ "clientId": "clïent" }),
        json!({ "clientId": CLIENT_ID, "clientSecret": "secret\u{202e}" }),
        json!({ "clientId": CLIENT_ID, "scopes": ["files read"] }),
        json!({ "clientId": CLIENT_ID, "issuer": "https://auth.example.com/?t=1" }),
        json!({ "clientId": CLIENT_ID, "unknown": true }),
    ] {
        let refused = paul.put(&oauth, body.clone()).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{body}: {}",
            refused.text()
        );
    }
    assert!(server.authorizations().is_empty());
    // A server whose authorization cannot be found: said, nothing kept.
    let hidden = serve_oauth_mcp_http(&authority, OAuthQuirk::NoMetadata).await;
    let other = add(&paul, mcp_body("Caché", "cache", &hidden.url, None)).await;
    let refused = paul
        .put(
            &format!("/admin/connectors/{other}/oauth"),
            json!({ "clientId": CLIENT_ID }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(
        refused.text().contains("introuvables"),
        "{}",
        refused.text()
    );
    assert_eq!(listed(&paul, &other).await["mcp"]["oauth"], Value::Null);
    // Not an MCP server added here, or none at all.
    let demo = paul.get("/admin/connectors").await.json()["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "demo_interventions")
        .cloned()
        .expect("demo");
    let demo_oauth = format!("/admin/connectors/{}/oauth", demo["id"].as_str().unwrap());
    let body = json!({ "clientId": CLIENT_ID });
    assert_eq!(
        paul.put(&demo_oauth, body.clone()).await.status,
        StatusCode::BAD_REQUEST
    );
    let absent = "/admin/connectors/00000000-0000-0000-0000-000000000001/oauth";
    assert_eq!(
        paul.put(absent, body.clone()).await.status,
        StatusCode::NOT_FOUND
    );
    // Only a company-wide connector administrator.
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.oauth").await;
    let partial = activate(&api, &mut partial).await;
    assert_eq!(
        partial.put(&oauth, body.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(partial.delete(&oauth).await.status, StatusCode::FORBIDDEN);

    let secret = json!({ "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET });
    assert_eq!(
        paul.put(&oauth, secret.clone()).await.status,
        StatusCode::OK
    );
    // No typed token with it.
    let typed = paul.patch(&instance, json!({ "token": TOKEN })).await;
    assert_eq!(typed.status, StatusCode::BAD_REQUEST);
    assert!(typed.text().contains("OAuth"), "{}", typed.text());
    // Not even with another address in the same change.
    let elsewhere = serve_demo_docs_http(None).await;
    let both = json!({ "url": elsewhere, "token": TOKEN });
    assert_eq!(
        paul.patch(&instance, both).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(listed(&paul, &id).await["mcp"]["url"], server.url);
    // No token to remove: nothing changes.
    let none = paul.patch(&instance, json!({ "token": null })).await;
    assert_eq!(none.status, StatusCode::OK);
    assert_eq!(
        listed(&paul, &id).await["mcp"]["oauth"]["clientId"],
        CLIENT_ID
    );
    // The same address keeps it; another one does not, nor its secret.
    let same = paul.patch(&instance, json!({ "url": server.url })).await;
    assert_eq!(same.status, StatusCode::OK);
    assert_eq!(
        listed(&paul, &id).await["mcp"]["oauth"]["clientId"],
        CLIENT_ID
    );
    let moved = paul.patch(&instance, json!({ "url": elsewhere })).await;
    assert_eq!(moved.status, StatusCode::OK);
    assert_eq!(listed(&paul, &id).await["mcp"]["oauth"], Value::Null);
    assert_eq!(sealed_secret(&api, &id).await, None);
    assert_eq!(last_update(&api, &id).await["oauth"], "removed");
    // The connector deleted, its client's secret with it.
    let back = paul.patch(&instance, json!({ "url": server.url })).await;
    assert_eq!(back.status, StatusCode::OK);
    assert_eq!(paul.put(&oauth, secret).await.status, StatusCode::OK);
    assert!(sealed_secret(&api, &id).await.is_some());
    assert_eq!(paul.delete(&instance).await.status, StatusCode::OK);
    assert_eq!(sealed_secret(&api, &id).await, None);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{other}"))
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_was_found_for_an_address_no_longer_its_own_is_not_kept() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::HeldMetadata).await;
    let id = add(&paul, mcp_body("Retenu", "held", &server.url, None)).await;
    let (instance, oauth) = (
        format!("/admin/connectors/{id}"),
        format!("/admin/connectors/{id}/oauth"),
    );
    let body = json!({ "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET });
    // Another administrator moves the server while its authorization is
    // looked for.
    let elsewhere = serve_demo_docs_http(None).await;
    let moving = async {
        server.hold.reached().await;
        let moved = paul.patch(&instance, json!({ "url": elsewhere })).await;
        server.hold.release();
        moved
    };
    let (declared, moved) = tokio::join!(paul.put(&oauth, body.clone()), moving);
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    assert_eq!(declared.status, StatusCode::CONFLICT, "{}", declared.text());
    assert!(declared.text().contains("a changé"), "{}", declared.text());
    assert_eq!(listed(&paul, &id).await["mcp"]["oauth"], Value::Null);
    assert_eq!(sealed_secret(&api, &id).await, None);
    let entries = audited(&api, &id).await;
    assert!(
        entries
            .iter()
            .all(|(action, _)| action != "connector.oauth_declared")
    );

    // Deleted meanwhile: answered as absent, nothing kept.
    let back = paul.patch(&instance, json!({ "url": server.url })).await;
    assert_eq!(back.status, StatusCode::OK);
    server.hold.hold_again();
    let deleting = async {
        server.hold.reached().await;
        let deleted = paul.delete(&instance).await;
        server.hold.release();
        deleted
    };
    let (declared, deleted) = tokio::join!(paul.put(&oauth, body), deleting);
    assert_eq!(deleted.status, StatusCode::OK);
    assert_eq!(
        declared.status,
        StatusCode::NOT_FOUND,
        "{}",
        declared.text()
    );
    assert_eq!(sealed_secret(&api, &id).await, None);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_declaration_that_can_no_longer_be_read_is_withdrawn_or_replaced() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let id = add(&paul, mcp_body("Abîmé", "broken", &server.url, None)).await;
    let (instance, oauth) = (
        format!("/admin/connectors/{id}"),
        format!("/admin/connectors/{id}/oauth"),
    );
    let spoil = async || {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        sqlx::query(
            "update connector_instances set config = config || '{\"oauth\": {\"issuer\": 1}}' \
             where id = $1::uuid",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
    };
    spoil().await;
    // Said as such, not as the operator's.
    let same = json!({ "url": server.url });
    let refused = paul.patch(&instance, same.clone()).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.text().contains("OAuth"), "{}", refused.text());
    // Withdrawn.
    assert_eq!(paul.delete(&oauth).await.status, StatusCode::OK);
    let repaired = paul.patch(&instance, same).await;
    assert_eq!(repaired.status, StatusCode::OK, "{}", repaired.text());
    // Or declared again over it.
    spoil().await;
    let declared = paul.put(&oauth, json!({ "clientId": CLIENT_ID })).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    let entries = audited(&api, &id).await;
    let (_, last) = entries.last().expect("audited");
    assert_eq!(last["replaced"], true);
    assert_eq!(paul.delete(&instance).await.status, StatusCode::OK);
}
