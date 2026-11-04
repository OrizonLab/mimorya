//! A session with an MCP server declared with OAuth (ADR-171): it is sent
//! the access token of the server's authorization, and starts without one
//! by saying it must be authorized; a token about to expire starts nothing,
//! the server not even asked; a token the server refuses, at the start or
//! on a call, says to authorize it again, the token masked.

mod support;

use std::time::Duration;

use mimorya_connectors::mcp::oauth::{ClientAuth, OAUTH_TO_AUTHORIZE, OAuthDeclaration};
use mimorya_connectors::mcp::{OAUTH_EXPIRED, OAUTH_OFFERED, OAUTH_REFUSED};
use mimorya_connectors::testing::{
    CLIENT_ID, FakeAuthorizationServer, OAuthQuirk, ProtectedMcp, serve_oauth_mcp_http,
};
use serde_json::{Value, json};
use support::mcp::{PRIVATE, call_as_reader, start_mcp_until};
use tokio::time::Instant;

/// The server's configuration once an administrator declared OAuth on it.
fn declared(server: &ProtectedMcp, authority: &FakeAuthorizationServer) -> Value {
    let declaration = OAuthDeclaration {
        issuer: authority.issuer.clone(),
        resource: server.url.clone(),
        authorization_endpoint: format!("{}/authorize", authority.issuer),
        token_endpoint: format!("{}/token", authority.issuer),
        names_issuer_on_return: false,
        client_id: CLIENT_ID.into(),
        client_auth: ClientAuth::ClientSecretPost,
        scopes: vec!["files:read".into()],
    };
    json!({ "transport": "http", "prefix": "docs", "url": server.url, "oauth": declaration })
}

fn in_an_hour() -> Option<Instant> {
    Some(Instant::now() + Duration::from_secs(3600))
}

#[tokio::test]
async fn a_declared_server_is_sent_its_authorization_s_token_and_none_other() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let config = declared(&server, &authority);
    // Not authorized: nothing starts, the server not asked.
    let none = start_mcp_until(config.clone(), None, None, PRIVATE)
        .await
        .err()
        .expect("refused");
    assert!(format!("{none:#}").contains(OAUTH_TO_AUTHORIZE), "{none:#}");
    // About to expire: nothing starts, the server not asked.
    let token = authority.issue(&server.url);
    let soon = Some(Instant::now() + Duration::from_secs(90));
    let expiring = start_mcp_until(config.clone(), Some(&token), soon, PRIVATE)
        .await
        .err()
        .expect("refused");
    assert!(
        format!("{expiring:#}").contains(OAUTH_EXPIRED),
        "{expiring:#}"
    );
    assert!(server.authorizations().is_empty());
    // Authorized: its tools, every request with its token.
    let runtime = start_mcp_until(config.clone(), Some(&token), in_an_hour(), PRIVATE)
        .await
        .expect("started");
    assert!(!runtime.tools.is_empty());
    assert!(!runtime.is_closed() && runtime.ended_at().is_none());
    let bearer = format!("Bearer {token}");
    let seen = server.authorizations();
    assert!(!seen.is_empty());
    assert!(
        seen.iter()
            .all(|given| given.as_deref() == Some(bearer.as_str()))
    );
    runtime.closer.expect("closer").close().await;
}

#[tokio::test]
async fn a_refused_authorization_says_to_authorize_again_without_the_token() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let other = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let config = declared(&server, &authority);
    // Issued for another server: refused at the start.
    let elsewhere = authority.issue(&other.url);
    let refused = start_mcp_until(config.clone(), Some(&elsewhere), in_an_hour(), PRIVATE)
        .await
        .err()
        .expect("refused");
    let said = format!("{refused:#}");
    assert!(
        said.contains(OAUTH_REFUSED) && !said.contains(OAUTH_OFFERED),
        "{said}"
    );
    assert!(!said.contains(&elsewhere), "{said}");
    // Revoked during the session: the call says so, the token masked.
    let token = authority.issue(&server.url);
    let runtime = start_mcp_until(config, Some(&token), in_an_hour(), PRIVATE)
        .await
        .expect("started");
    authority.revoke_all();
    let search = json!({ "query": "circulateur" });
    let result = call_as_reader(&runtime, "mcp_docs_search_procedures", search).await;
    let shown = result.content.to_string();
    assert!(result.is_error, "{shown}");
    assert!(shown.contains(OAUTH_REFUSED), "{shown}");
    assert!(!shown.contains(&token), "{shown}");
    runtime.closer.expect("closer").close().await;
}
