//! An MCP server's access token renewed with its refresh token (ADR-171):
//! the server issues new tokens for the same resource, the refresh token
//! rotated; one rotated away and used again is refused by its error code
//! alone, and the whole grant is then revoked; a server that cannot be
//! reached is told apart from a refusal; the client proves itself as the
//! declaration says.

mod support;

use mimorya_connectors::mcp::oauth::{
    CodeExchange, Departure, OAuthDeclaration, RenewalFailure, authorization_url, discover,
    exchange_code, refresh_tokens,
};
use mimorya_connectors::testing::{
    CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http,
};
use support::mcp::PRIVATE;
use support::oauth::{BACK, VERIFIER, challenge_of, sent_back};

/// A declaration at the simulated server and the tokens of a first
/// authorization: its access token, its refresh token, the server's address.
async fn authorized(
    authority: &FakeAuthorizationServer,
    secret: bool,
) -> (OAuthDeclaration, String, String) {
    let server = serve_oauth_mcp_http(authority, OAuthQuirk::None).await;
    let found = discover(&server.url, None, PRIVATE).await.expect("found");
    let declaration = OAuthDeclaration::new(found, CLIENT_ID, secret, None).expect("declared");
    let challenge = challenge_of(VERIFIER);
    let departure = Departure {
        redirect_uri: BACK,
        state: "etat",
        code_challenge: &challenge,
    };
    let back = sent_back(&authorization_url(&declaration, departure).expect("url")).await;
    let exchange = CodeExchange {
        code: &back["code"],
        verifier: VERIFIER,
        redirect_uri: BACK,
        client_secret: secret.then_some(CLIENT_SECRET),
    };
    let granted = exchange_code(&declaration, exchange, PRIVATE)
        .await
        .expect("granted");
    let refresh = granted.refresh_token.expect("renewable");
    (declaration, refresh, server.url)
}

#[tokio::test]
async fn a_refresh_token_brings_new_tokens_and_cannot_be_used_twice() {
    let authority = FakeAuthorizationServer::start().await;
    let (declaration, first, url) = authorized(&authority, true).await;
    let renewed = refresh_tokens(&declaration, &first, Some(CLIENT_SECRET), PRIVATE)
        .await
        .expect("renewed");
    assert!(authority.accepts(&renewed.access_token, &url));
    assert_eq!(renewed.expires_in, Some(3600));
    let second = renewed.refresh_token.expect("rotated");
    assert_ne!(second, first);
    // Asked for the declared resource, the client proving itself as declared.
    let asked = authority.requests();
    let last = asked.last().expect("asked");
    assert_eq!(last.params["grant_type"], "refresh_token");
    assert_eq!(last.params["resource"], url);
    assert_eq!(last.params["client_secret"], CLIENT_SECRET);
    // The rotated-away token used again: refused, the grant revoked.
    let again = refresh_tokens(&declaration, &first, Some(CLIENT_SECRET), PRIVATE).await;
    let Err(RenewalFailure::Refused(said)) = again else {
        panic!("refused");
    };
    assert!(
        said.contains("(invalid_grant)") && !said.contains(&first),
        "{said}"
    );
    assert!(!authority.accepts(&renewed.access_token, &url));
    let revoked = refresh_tokens(&declaration, &second, Some(CLIENT_SECRET), PRIVATE).await;
    assert!(matches!(revoked, Err(RenewalFailure::Refused(_))));
}

#[tokio::test]
async fn a_public_client_renews_with_its_identifier_and_an_absent_server_is_not_a_refusal() {
    let authority = FakeAuthorizationServer::start().await;
    let (declaration, refresh, url) = authorized(&authority, false).await;
    let renewed = refresh_tokens(&declaration, &refresh, None, PRIVATE)
        .await
        .expect("renewed");
    assert!(authority.accepts(&renewed.access_token, &url));
    let asked = authority.requests();
    let last = asked.last().expect("asked");
    assert_eq!(last.params["client_id"], CLIENT_ID);
    assert!(!last.params.contains_key("client_secret"));
    // Its token endpoint gone: tried again later, not lost.
    let gone = OAuthDeclaration {
        token_endpoint: "http://127.0.0.1:1/token".into(),
        ..declaration
    };
    let unreachable = refresh_tokens(&gone, &refresh, None, PRIVATE).await;
    let Err(RenewalFailure::Unavailable(said)) = unreachable else {
        panic!("not reached, not refused");
    };
    assert!(!said.contains(&refresh), "{said}");
}
