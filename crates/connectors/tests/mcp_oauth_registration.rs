//! The platform registered as a client at an MCP server's authorization
//! server (RFC 7591, ADR-173): at the registration endpoint its metadata
//! names, for the platform's return address alone, with a secret when the
//! platform can keep one, else as a public client; the client registered is
//! then authorized and its code exchanged like a typed one. A server that
//! offers no registration, asks for an initial token, refuses or answers
//! what the platform cannot keep is said, never with a secret.

mod support;

use mimorya_connectors::mcp::oauth::{
    CLIENT_NAME, ClientAuth, CodeExchange, Departure, Discovery, NO_REGISTRATION_ENDPOINT,
    OAuthDeclaration, REGISTRATION_TOKEN_NEEDED, asked_scopes, authorization_url, discover,
    exchange_code, register_client,
};
use mimorya_connectors::testing::{FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http};
use support::mcp::PRIVATE;
use support::oauth::{BACK, VERIFIER, challenge_of, sent_back};

/// What the discovery finds of a server `authority` protects, and the
/// server's address.
async fn found(authority: &FakeAuthorizationServer) -> (Discovery, String) {
    let server = serve_oauth_mcp_http(authority, OAuthQuirk::None).await;
    let found = discover(&server.url, None, PRIVATE).await.expect("found");
    (found, server.url)
}

#[tokio::test]
async fn a_registered_client_is_authorized_like_a_typed_one() {
    let authority = FakeAuthorizationServer::start().await;
    let (found, url) = found(&authority).await;
    let scopes = asked_scopes(&found, None).expect("scopes");
    let registration = register_client(&found.server, BACK, &scopes, true, PRIVATE)
        .await
        .expect("registered");
    // Asked for the platform's return address alone, with a secret.
    let requests = authority.requests();
    let asked = &requests.last().expect("asked").params;
    assert_eq!(asked["client_name"], CLIENT_NAME);
    assert_eq!(asked["redirect_uris"], BACK);
    assert_eq!(asked["grant_types"], "authorization_code refresh_token");
    assert_eq!(asked["response_types"], "code");
    assert_eq!(asked["token_endpoint_auth_method"], "client_secret_post");
    assert_eq!(asked["scope"], scopes.join(" "));
    assert_eq!(authority.registered(), vec![registration.client_id.clone()]);
    let secret = registration.client_secret.clone().expect("secret");
    assert_eq!(registration.client_auth, ClientAuth::ClientSecretPost);
    // Declared, authorized there and back, its code exchanged.
    let declaration = OAuthDeclaration::registered(found, &registration, scopes);
    assert_eq!(declaration.client_id, registration.client_id);
    assert!(declaration.registered.is_some());
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
        client_secret: Some(&secret),
    };
    let granted = exchange_code(&declaration, exchange, PRIVATE)
        .await
        .expect("granted");
    assert!(authority.accepts(&granted.access_token, &url));
}

#[tokio::test]
async fn a_platform_that_cannot_keep_a_secret_registers_a_public_client() {
    let authority = FakeAuthorizationServer::start().await;
    let (found, _) = found(&authority).await;
    let registration = register_client(&found.server, BACK, &[], false, PRIVATE)
        .await
        .expect("registered");
    assert_eq!(registration.client_auth, ClientAuth::None);
    assert!(registration.client_secret.is_none());
    let requests = authority.requests();
    let asked = &requests.last().expect("asked").params;
    assert_eq!(asked["token_endpoint_auth_method"], "none");
    assert!(asked["scope"].is_empty());
    // A server that takes no public client: said.
    authority.set(|settings| {
        settings.auth_methods = vec!["client_secret_basic".into()];
    });
    let (found, _) = self::found(&authority).await;
    let refused = register_client(&found.server, BACK, &[], false, PRIVATE).await;
    assert!(
        refused
            .err()
            .expect("refused")
            .contains("clé de chiffrement")
    );
}

#[tokio::test]
async fn what_keeps_a_registration_from_being_kept_is_said_without_a_secret() {
    // No registration endpoint.
    let closed = FakeAuthorizationServer::start().await;
    closed.set(|settings| settings.registration = false);
    let (found, _) = found(&closed).await;
    let refused = register_client(&found.server, BACK, &[], true, PRIVATE).await;
    assert_eq!(refused.err().as_deref(), Some(NO_REGISTRATION_ENDPOINT));
    // An initial access token asked for.
    let guarded = FakeAuthorizationServer::start().await;
    guarded.set(|settings| settings.registration_token = true);
    let (found, _) = self::found(&guarded).await;
    let refused = register_client(&found.server, BACK, &[], true, PRIVATE).await;
    assert_eq!(refused.err().as_deref(), Some(REGISTRATION_TOKEN_NEEDED));
    assert!(guarded.registered().is_empty());
    // A secret that would not last a day.
    let brief = FakeAuthorizationServer::start().await;
    brief.set(|settings| settings.secret_lifetime = Some(3600));
    let (found, _) = self::found(&brief).await;
    let refused = register_client(&found.server, BACK, &[], true, PRIVATE)
        .await
        .err()
        .expect("refused");
    assert!(refused.contains("avant un jour"), "{refused}");
    assert!(!refused.contains("secret-client"), "{refused}");
    // A return address the server does not take: refused by its code.
    let lasting = FakeAuthorizationServer::start().await;
    lasting.set(|settings| settings.secret_lifetime = Some(30 * 24 * 3600));
    let (found, _) = self::found(&lasting).await;
    let registration = register_client(&found.server, BACK, &[], true, PRIVATE)
        .await
        .expect("registered");
    assert!(registration.secret_expires_at.is_some());
    let refused = register_client(&found.server, "", &[], true, PRIVATE).await;
    assert_eq!(
        refused.err().as_deref(),
        Some("Enregistrement refusé par le serveur d’autorisation (invalid_redirect_uri)")
    );
}
