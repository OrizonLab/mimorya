//! An MCP server authorized (ADR-171): the browser sent where the
//! declaration says, with PKCE, for the resource, comes back with a code
//! the platform exchanges for tokens, the client proving itself as the
//! declaration says; a code spent or a wrong verifier is refused by its
//! error code alone; tokens the platform could not mask, or that expire
//! without a refresh token, are refused; the network rules hold.

mod support;

use mimorya_connectors::mcp::oauth::{
    CodeExchange, Departure, OAuthDeclaration, authorization_url, discover, exchange_code,
};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::testing::{
    CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http,
};
use support::mcp::PRIVATE;
use support::oauth::{BACK, VERIFIER, challenge_of, sent_back};

/// The declaration of the simulated server's client, with its secret or
/// without, and the server's address.
async fn declared(authority: &FakeAuthorizationServer, secret: bool) -> (OAuthDeclaration, String) {
    let server = serve_oauth_mcp_http(authority, OAuthQuirk::None).await;
    let found = discover(&server.url, None, PRIVATE).await.expect("found");
    let declaration = OAuthDeclaration::new(found, CLIENT_ID, secret, None).expect("declared");
    (declaration, server.url)
}

/// The code the browser comes back with, the attempt's state checked.
async fn code(declaration: &OAuthDeclaration) -> String {
    let challenge = challenge_of(VERIFIER);
    let departure = Departure {
        redirect_uri: BACK,
        state: "etat-de-la-tentative",
        code_challenge: &challenge,
        nonce: None,
    };
    let url = authorization_url(declaration, departure).expect("url");
    let back = sent_back(&url).await;
    assert_eq!(back["state"], "etat-de-la-tentative");
    back["code"].clone()
}

fn exchange<'a>(code: &'a str, secret: Option<&'a str>) -> CodeExchange<'a> {
    CodeExchange {
        code,
        verifier: VERIFIER,
        redirect_uri: BACK,
        client_secret: secret,
    }
}

#[tokio::test]
async fn the_code_brought_back_is_exchanged_for_tokens_for_the_server() {
    let authority = FakeAuthorizationServer::start().await;
    let (declaration, url) = declared(&authority, true).await;
    let code = code(&declaration).await;
    let granted = exchange_code(&declaration, exchange(&code, Some(CLIENT_SECRET)), PRIVATE)
        .await
        .expect("granted");
    assert!(authority.accepts(&granted.access_token, &url));
    assert_eq!(granted.expires_in, Some(3600));
    assert!(granted.refresh_token.is_some());
    assert_eq!(granted.scopes, Some(vec!["files:read".to_string()]));
    // The secret went in the body, as the server lists first.
    let asked = authority.requests();
    let token = asked.iter().find(|r| r.endpoint == "token").expect("asked");
    assert_eq!(token.params["client_secret"], CLIENT_SECRET);
    assert_eq!(token.params["resource"], url);
    // Spent: refused by its error code, the code never said.
    let again = exchange_code(&declaration, exchange(&code, Some(CLIENT_SECRET)), PRIVATE).await;
    let refused = again.err().expect("refused");
    assert!(
        refused.contains("(invalid_grant)") && !refused.contains(&code),
        "{refused}"
    );
    // Another verifier than the challenge's: refused.
    let code = self::code(&declaration).await;
    let wrong = CodeExchange {
        verifier: "autre-verificateur-pkce-0123456789-abcdefghijklmnop",
        ..exchange(&code, Some(CLIENT_SECRET))
    };
    let refused = exchange_code(&declaration, wrong, PRIVATE).await.err();
    assert!(refused.expect("refused").contains("invalid_grant"));
}

#[tokio::test]
async fn the_client_proves_itself_as_declared() {
    let authority = FakeAuthorizationServer::start().await;
    // Basic only.
    authority.set(|settings| settings.auth_methods = vec!["client_secret_basic".into()]);
    let (basic, _) = declared(&authority, true).await;
    let code = code(&basic).await;
    let granted = exchange_code(&basic, exchange(&code, Some(CLIENT_SECRET)), PRIVATE).await;
    assert!(granted.is_ok());
    let asked = authority.requests();
    let token = asked
        .iter()
        .rev()
        .find(|r| r.endpoint == "token")
        .expect("asked");
    assert!(token.basic.is_some() && !token.params.contains_key("client_secret"));
    // A public client: its identifier alone.
    authority.set(|settings| settings.auth_methods = vec!["none".into()]);
    let (public, _) = declared(&authority, false).await;
    let code = self::code(&public).await;
    assert!(
        exchange_code(&public, exchange(&code, None), PRIVATE)
            .await
            .is_ok()
    );
    // A secret declared but gone: said, nothing asked.
    let (with_secret, _) = declared(&FakeAuthorizationServer::start().await, true).await;
    let refused = exchange_code(&with_secret, exchange("code", None), PRIVATE).await;
    assert!(
        refused
            .err()
            .expect("refused")
            .contains("Secret du client absent")
    );
}

#[tokio::test]
async fn tokens_the_platform_could_not_keep_are_refused() {
    let authority = FakeAuthorizationServer::start().await;
    let (declaration, _) = declared(&authority, true).await;
    // An access token that expires, without a refresh token.
    authority.set(|settings| settings.refresh = false);
    let code = code(&declaration).await;
    let refused = exchange_code(&declaration, exchange(&code, Some(CLIENT_SECRET)), PRIVATE).await;
    assert!(refused.err().expect("refused").contains("renouvellement"));
    // One that does not expire needs none.
    authority.set(|settings| settings.lifetime = None);
    let code = self::code(&declaration).await;
    let lasting = exchange_code(&declaration, exchange(&code, Some(CLIENT_SECRET)), PRIVATE).await;
    let lasting = lasting.expect("granted");
    assert_eq!((lasting.expires_in, lasting.refresh_token), (None, None));
    // One the platform could not mask.
    authority.set(|settings| settings.off_alphabet = true);
    let code = self::code(&declaration).await;
    let refused = exchange_code(&declaration, exchange(&code, Some(CLIENT_SECRET)), PRIVATE).await;
    assert!(refused.err().expect("refused").contains("RFC 6750"));
}

#[tokio::test]
async fn nothing_is_asked_where_the_network_rules_forbid() {
    let authority = FakeAuthorizationServer::start().await;
    let (declaration, _) = declared(&authority, true).await;
    let public_only = NetworkPolicy::default();
    let refused = exchange_code(
        &declaration,
        exchange("code", Some(CLIENT_SECRET)),
        public_only,
    )
    .await;
    assert!(refused.err().expect("refused").contains("https://"));
    assert!(authority.requests().iter().all(|r| r.endpoint != "token"));
}
