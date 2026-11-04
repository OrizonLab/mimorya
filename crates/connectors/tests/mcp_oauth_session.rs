//! A session with an MCP server declared with OAuth (ADR-171): it is sent
//! the access token of the server's authorization, however long the
//! authorization server writes it, and starts without one, or with one that
//! could not be sent as kept, by saying it must be authorized again; a
//! token about to expire starts nothing, the server not even asked, nor one
//! a slow start used up; a session is retired a minute before its token
//! expires; a token the server refuses, at the start or on a call, says to
//! authorize it again, the token masked.

mod support;

use std::time::Duration;

use mimorya_connectors::mcp::oauth::{ClientAuth, OAUTH_TO_AUTHORIZE, OAuthDeclaration};
use mimorya_connectors::mcp::{
    MIN_TOKEN_LIFE, OAUTH_EXPIRED, OAUTH_OFFERED, OAUTH_REFUSED, OAUTH_UNUSABLE, RETIRE_BEFORE,
};
use mimorya_connectors::outbound::MAX_TOKEN_CHARS;
use mimorya_connectors::testing::{
    CLIENT_ID, FakeAuthorizationServer, LONG_TOKEN_CHARS, OAuthQuirk, ProtectedMcp, SLOW_ANSWER,
    serve_oauth_mcp_http,
};
use serde_json::{Value, json};
use support::mcp::{PRIVATE, call_as_reader, start_mcp_until};
use tokio::time::Instant;

/// The server's configuration once an administrator declared OAuth on it.
fn declared(server: &ProtectedMcp, authority: &FakeAuthorizationServer) -> Value {
    let declaration = OAuthDeclaration {
        revocation_endpoint: None,
        registered: None,
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
    // One that could not be sent as kept: nothing starts.
    let unusable = Some("jeton,illisible");
    let unusable = start_mcp_until(config.clone(), unusable, in_an_hour(), PRIVATE)
        .await
        .err()
        .expect("refused");
    assert!(
        format!("{unusable:#}").contains(OAUTH_UNUSABLE),
        "{unusable:#}"
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

#[tokio::test]
async fn a_long_token_is_sent_as_issued() {
    let authority = FakeAuthorizationServer::start().await;
    authority.set(|settings| settings.long_tokens = true);
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let token = authority.issue(&server.url);
    assert!(token.len() == LONG_TOKEN_CHARS && token.len() > MAX_TOKEN_CHARS);
    let config = declared(&server, &authority);
    let runtime = start_mcp_until(config, Some(&token), in_an_hour(), PRIVATE)
        .await
        .expect("started");
    assert!(!runtime.tools.is_empty());
    let bearer = format!("Bearer {token}");
    assert!(server.authorizations().contains(&Some(bearer)));
    runtime.closer.expect("closer").close().await;
}

#[tokio::test]
async fn a_session_is_retired_a_minute_before_its_token_expires() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let token = authority.issue(&server.url);
    let until = Instant::now() + Duration::from_secs(200);
    let config = declared(&server, &authority);
    let runtime = start_mcp_until(config, Some(&token), Some(until), PRIVATE)
        .await
        .expect("started");
    // From here the clock moves only as the test says.
    tokio::time::pause();
    let retired = until - RETIRE_BEFORE;
    tokio::time::advance(retired - Instant::now() - Duration::from_secs(1)).await;
    assert!(!runtime.is_closed() && runtime.ended_at().is_none());
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(runtime.is_closed());
    assert_eq!(runtime.ended_at(), Some(retired));
    runtime.closer.expect("closer").close().await;
}

#[tokio::test]
async fn a_token_a_slow_start_used_up_starts_nothing() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::SlowStart).await;
    let token = authority.issue(&server.url);
    // Enough left when the start begins, no longer once it is done.
    let until = Instant::now() + MIN_TOKEN_LIFE + SLOW_ANSWER / 2;
    let config = declared(&server, &authority);
    let refused = start_mcp_until(config, Some(&token), Some(until), PRIVATE)
        .await
        .err()
        .expect("refused");
    assert!(
        format!("{refused:#}").contains(OAUTH_EXPIRED),
        "{refused:#}"
    );
    // The server was reached: the second look refused it.
    assert!(!server.authorizations().is_empty());
}
