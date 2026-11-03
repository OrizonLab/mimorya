//! Finding where an MCP server is authorized (ADR-171): through its
//! refusal and its metadata, on its own origin only, at the root as well;
//! the administrator's issuer, which must be one the server names, chooses
//! among several; without metadata, that issuer is asked; a server that
//! misleads, an authorization server that names another issuer, or one the
//! network rules forbid, is refused, the reason said.

mod support;

use mimorya_connectors::mcp::oauth::{ClientAuth, discover};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::testing::{FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http};
use support::mcp::PRIVATE;

#[tokio::test]
async fn the_refusal_and_metadata_lead_to_the_authorization_server() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let found = discover(&server.url, None, PRIVATE).await.expect("found");
    assert_eq!(found.resource, server.url);
    assert_eq!(found.issuer, authority.issuer);
    assert_eq!(
        found.server.token_endpoint,
        format!("{}/token", authority.issuer)
    );
    assert_eq!(
        found.server.client_auth(true),
        Ok(ClientAuth::ClientSecretPost)
    );
    // The refusal's scopes first.
    assert_eq!(found.suggested_scopes, ["files:read"]);
    // Named as the server names it: the same.
    let named = discover(&server.url, Some(&authority.issuer), PRIVATE).await;
    assert_eq!(named.expect("found").issuer, authority.issuer);
    // The probe went without a token.
    assert_eq!(server.authorizations().first(), Some(&None));
}

#[tokio::test]
async fn metadata_at_the_root_names_the_origin_as_the_resource() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::RootMetadata).await;
    let found = discover(&server.url, None, PRIVATE).await.expect("found");
    let origin = server.url.trim_end_matches("/mcp");
    assert_eq!(found.resource, origin);
    assert_eq!(found.issuer, authority.issuer);
    // Its metadata's scopes, the refusal naming none.
    assert_eq!(found.suggested_scopes, ["files:read"]);
}

#[tokio::test]
async fn a_challenge_naming_another_origin_is_not_followed() {
    let authority = FakeAuthorizationServer::start().await;
    // The challenge names metadata elsewhere: the well-known location of
    // the server itself is read instead.
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::ForeignMetadata).await;
    let found = discover(&server.url, None, PRIVATE).await.expect("found");
    assert_eq!(found.issuer, authority.issuer);
    // Neither suggested scopes from a challenge that named another origin.
    assert_eq!(found.suggested_scopes, ["files:read"]);
}

#[tokio::test]
async fn several_authorization_servers_wait_for_the_administrator_s_choice() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::TwoServers).await;
    let refused = discover(&server.url, None, PRIVATE)
        .await
        .expect_err("ambiguous");
    assert!(
        refused.contains("plusieurs serveurs d’autorisation"),
        "{refused}"
    );
    assert!(refused.contains(&authority.issuer), "{refused}");
    let chosen = discover(&server.url, Some(&authority.issuer), PRIVATE).await;
    assert_eq!(chosen.expect("chosen").issuer, authority.issuer);
    // An issuer the server does not name: refused.
    let elsewhere = FakeAuthorizationServer::start().await;
    let refused = discover(&server.url, Some(&elsewhere.issuer), PRIVATE).await;
    assert!(refused.expect_err("refused").contains("ne nomme pas"));
}

#[tokio::test]
async fn without_metadata_the_administrator_names_the_issuer() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::NoMetadata).await;
    // The server's origin is no authorization server.
    let refused = discover(&server.url, None, PRIVATE)
        .await
        .expect_err("refused");
    assert!(refused.contains("introuvables"), "{refused}");
    let found = discover(&server.url, Some(&authority.issuer), PRIVATE).await;
    let found = found.expect("found");
    assert_eq!(found.resource, server.url);
    assert_eq!(found.issuer, authority.issuer);
    assert!(found.suggested_scopes.is_empty());
}

#[tokio::test]
async fn misleading_metadata_is_refused_and_said() {
    let authority = FakeAuthorizationServer::start().await;
    let other = serve_oauth_mcp_http(&authority, OAuthQuirk::OtherResource).await;
    let refused = discover(&other.url, None, PRIVATE)
        .await
        .expect_err("refused");
    assert!(refused.contains("une autre ressource"), "{refused}");
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    authority.set(|settings| settings.wrong_issuer = true);
    let refused = discover(&server.url, None, PRIVATE)
        .await
        .expect_err("refused");
    assert!(refused.contains("un autre émetteur"), "{refused}");
}

#[tokio::test]
async fn nothing_is_asked_where_the_network_rules_forbid() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let public_only = NetworkPolicy::default();
    let refused = discover(&server.url, None, public_only)
        .await
        .expect_err("refused");
    assert!(refused.contains("https://"), "{refused}");
    // Not even knocked at.
    assert!(server.authorizations().is_empty());
    // An issuer RFC 8414 does not allow: refused before anything is asked.
    let issuer = "https://auth.example.com/?tenant=1";
    let refused = discover(&server.url, Some(issuer), PRIVATE).await;
    assert!(refused.expect_err("refused").contains("RFC 8414"));
    assert!(server.authorizations().is_empty());
}
