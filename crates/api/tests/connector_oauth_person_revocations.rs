//! The people's own OAuth authorizations of an MCP server (ADR-174) go with
//! the declaration they were given under, their tokens revoked at the
//! authorization server (ADR-172), audited together without a token nor a
//! name: when OAuth is declared again or withdrawn, when the server's
//! address changes and when the connector is deleted; another connector's
//! stay.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{
    CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http,
};
use mimorya_core::demo::user;
use serde_json::json;
use support::connector_oauth::declared;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{person_authorized, person_count, person_revocations};
use support::{Client, TestApi};

/// A server each person authorizes, declared, with Marc's and Léa's own
/// authorizations: its id, its resource and their tokens.
async fn authorized_by_two(
    api: &TestApi,
    paul: &Client<'_>,
    authority: &FakeAuthorizationServer,
    prefix: &str,
) -> (String, String, [String; 2]) {
    let (_, id) = declared(paul, authority, prefix).await;
    let each = paul
        .put(
            &format!("/admin/connectors/{id}/oauth/holder"),
            json!({ "perPerson": true }),
        )
        .await;
    assert_eq!(each.status, StatusCode::OK, "{}", each.text());
    let resource = listed(paul, &id).await["mcp"]["oauth"]["resource"]
        .as_str()
        .unwrap()
        .to_string();
    let marc = person_authorized(api, &id, user::MARC, authority, &resource).await;
    let lea = person_authorized(api, &id, user::LEA, authority, &resource).await;
    (id, resource, [marc, lea])
}

/// The tokens no longer good, revoked and audited together.
async fn revoked(
    api: &TestApi,
    authority: &FakeAuthorizationServer,
    id: &str,
    resource: &str,
    tokens: &[String; 2],
) {
    assert_eq!(person_count(api, id).await, 0);
    for token in tokens {
        assert!(!authority.accepts(token, resource));
    }
    let audited = person_revocations(api, id).await;
    let last = audited.last().expect("audited");
    assert_eq!(last.0, "success", "{audited:?}");
    assert_eq!(last.2, json!({ "revoked": 2, "failed": 0 }));
    let said = format!("{audited:?}");
    assert!(tokens.iter().all(|token| !said.contains(token.as_str())));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_people_s_authorizations_go_with_their_declaration() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let declaration = json!({
        "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET, "perPerson": true,
    });
    // Another connector's stay whatever happens to these.
    let (other, other_resource, other_tokens) =
        authorized_by_two(&api, &paul, &authority, "autre").await;

    // OAuth declared again.
    let (id, resource, tokens) = authorized_by_two(&api, &paul, &authority, "redit").await;
    let again = paul
        .put(
            &format!("/admin/connectors/{id}/oauth"),
            declaration.clone(),
        )
        .await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    assert_eq!(again.json()["personAuthorizations"], 0);
    revoked(&api, &authority, &id, &resource, &tokens).await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );

    // OAuth withdrawn.
    let (id, resource, tokens) = authorized_by_two(&api, &paul, &authority, "retire").await;
    let withdrawn = paul.delete(&format!("/admin/connectors/{id}/oauth")).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    revoked(&api, &authority, &id, &resource, &tokens).await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );

    // The server's address changed.
    let (id, resource, tokens) = authorized_by_two(&api, &paul, &authority, "demenage").await;
    let elsewhere = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let moved = paul
        .patch(
            &format!("/admin/connectors/{id}"),
            json!({ "url": elsewhere.url }),
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    revoked(&api, &authority, &id, &resource, &tokens).await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );

    // The connector deleted.
    let (id, resource, tokens) = authorized_by_two(&api, &paul, &authority, "supprime").await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
    revoked(&api, &authority, &id, &resource, &tokens).await;

    // The other connector's are still kept and good.
    assert_eq!(person_count(&api, &other).await, 2);
    assert!(
        other_tokens
            .iter()
            .all(|t| authority.accepts(t, &other_resource))
    );
    assert!(person_revocations(&api, &other).await.is_empty());
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{other}"))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(person_count(&api, &other).await, 0);
}
