//! An MCP server protected by OAuth (ADR-171): it says so when refused, a
//! typed token told apart; it opens to a token issued for its very address
//! and to no other; its challenge, even on two header lines, and its
//! metadata lead to its authorization server only when they come from its
//! own origin and name it. How the simulated authorization server behaves:
//! `mcp_oauth_authority.rs`.

mod support;

use mimorya_connectors::mcp::oauth::{
    ClientAuth, bearer_challenge, metadata_locations, protected_resource_locations,
    read_protected_resource, read_server_metadata, same_origin,
};
use mimorya_connectors::mcp::{OAUTH_OFFERED, OAUTH_REQUIRED};
use mimorya_connectors::outbound::ACCESS_DENIED;
use mimorya_connectors::testing::{FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http};
use support::mcp::{PRIVATE, http, start_mcp};
use support::oauth::get;

async fn failure(url: &str, token: Option<&str>) -> String {
    let error = start_mcp(http(url), token, PRIVATE)
        .await
        .err()
        .expect("refused");
    format!("{error:#}")
}

#[tokio::test]
async fn a_protected_server_says_it_wants_an_oauth_authorization() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let refused = failure(&server.url, None).await;
    assert!(refused.contains(OAUTH_REQUIRED), "{refused}");
    // A typed token refused: it may be the server's own kind, gone stale.
    let typed = failure(&server.url, Some("jeton-saisi-a-la-main-0123")).await;
    assert!(
        typed.contains(OAUTH_OFFERED) && !typed.contains(OAUTH_REQUIRED),
        "{typed}"
    );
    // The challenge on its own header line, after another scheme's.
    let split = serve_oauth_mcp_http(&authority, OAuthQuirk::SplitChallenge).await;
    let refused = failure(&split.url, None).await;
    assert!(refused.contains(OAUTH_REQUIRED), "{refused}");
    // A refusal naming no metadata is an ordinary one.
    let plain = serve_oauth_mcp_http(&authority, OAuthQuirk::NoMetadata).await;
    let refused = failure(&plain.url, None).await;
    assert!(
        refused.contains(ACCESS_DENIED) && !refused.contains("OAuth"),
        "{refused}"
    );
}

#[tokio::test]
async fn a_token_issued_for_the_server_opens_it_and_one_for_another_does_not() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let other = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let token = authority.issue(&server.url);
    let runtime = start_mcp(http(&server.url), Some(&token), PRIVATE)
        .await
        .expect("started");
    assert!(!runtime.tools.is_empty());
    let seen = server.authorizations();
    assert!(seen.contains(&Some(format!("Bearer {token}"))));
    // Issued for the other server: refused by this one.
    let elsewhere = authority.issue(&other.url);
    let refused = failure(&server.url, Some(&elsewhere)).await;
    assert!(refused.contains(OAUTH_OFFERED), "{refused}");
    // Revoked at the authorization server: refused.
    authority.revoke_all();
    let refused = failure(&server.url, Some(&token)).await;
    assert!(refused.contains(OAUTH_OFFERED), "{refused}");
    // A server that refuses every token: refused, even a good one.
    let closed = serve_oauth_mcp_http(&authority, OAuthQuirk::RefusesAll).await;
    let good = authority.issue(&closed.url);
    let refused = failure(&closed.url, Some(&good)).await;
    assert!(refused.contains(OAUTH_OFFERED), "{refused}");
}

#[tokio::test]
async fn the_challenge_and_metadata_lead_to_the_authorization_server() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let (status, header, _) = get(&server.url).await;
    assert_eq!(status, 401);
    let challenge = bearer_challenge(&header.expect("challenge")).expect("read");
    let metadata = challenge.resource_metadata.expect("metadata named");
    assert!(same_origin(&server.url, &metadata));
    assert_eq!(protected_resource_locations(&server.url)[0], metadata);
    assert_eq!(challenge.scope.as_deref(), Some("files:read"));
    let (_, _, protected) = get(&metadata).await;
    let resource =
        read_protected_resource(&protected, &metadata, &server.url, PRIVATE).expect("metadata");
    assert_eq!(
        resource.authorization_servers,
        std::slice::from_ref(&authority.issuer)
    );
    let issuer = &resource.authorization_servers[0];
    let (status, _, served) = get(&metadata_locations(issuer)[0]).await;
    assert_eq!(status, 200);
    let found = read_server_metadata(&served, issuer, PRIVATE).expect("authorization server");
    assert_eq!(found.token_endpoint, format!("{issuer}/token"));
    assert_eq!(found.client_auth(true), Ok(ClientAuth::ClientSecretPost));
    assert_eq!(found.client_auth(false), Ok(ClientAuth::None));
    assert!(found.offers_offline_access());
    // Without the private network the operator allows: its local
    // authorization server is refused, and said so.
    let refused = read_protected_resource(&protected, &metadata, &server.url, Default::default());
    let refused = refused.expect_err("refused");
    assert!(
        refused.contains("Adresse en https:// attendue"),
        "{refused}"
    );
}

#[tokio::test]
async fn misleading_metadata_is_not_followed() {
    let authority = FakeAuthorizationServer::start().await;
    // A challenge naming metadata elsewhere.
    let foreign = serve_oauth_mcp_http(&authority, OAuthQuirk::ForeignMetadata).await;
    let (_, header, _) = get(&foreign.url).await;
    let named = bearer_challenge(&header.expect("challenge")).and_then(|c| c.resource_metadata);
    assert!(!same_origin(&foreign.url, &named.expect("named")));
    // Metadata naming another resource.
    let other = serve_oauth_mcp_http(&authority, OAuthQuirk::OtherResource).await;
    let location = &protected_resource_locations(&other.url)[0];
    let (_, _, body) = get(location).await;
    let refused = read_protected_resource(&body, location, &other.url, PRIVATE);
    let refused = refused.expect_err("refused");
    assert!(refused.contains("une autre ressource"), "{refused}");
    // Two authorization servers: both read, the administrator to choose.
    let two = serve_oauth_mcp_http(&authority, OAuthQuirk::TwoServers).await;
    let location = &protected_resource_locations(&two.url)[0];
    let (_, _, body) = get(location).await;
    let read = read_protected_resource(&body, location, &two.url, PRIVATE).expect("read");
    assert_eq!(read.authorization_servers.len(), 2);
    // No metadata at all, nor named.
    let none = serve_oauth_mcp_http(&authority, OAuthQuirk::NoMetadata).await;
    let (_, header, _) = get(&none.url).await;
    let named = bearer_challenge(&header.expect("challenge")).and_then(|c| c.resource_metadata);
    assert_eq!(named, None);
    assert_eq!(
        get(&protected_resource_locations(&none.url)[0]).await.0,
        404
    );
    // An authorization server naming another issuer.
    authority.set(|settings| settings.wrong_issuer = true);
    let (_, _, body) = get(&metadata_locations(&authority.issuer)[0]).await;
    let refused = read_server_metadata(&body, &authority.issuer, PRIVATE).expect_err("refused");
    assert!(refused.contains("un autre émetteur"), "{refused}");
}
