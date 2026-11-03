//! The simulated authorization server the OAuth lots rely on (ADR-171),
//! as strict as a real one must be expected to be: the client proves
//! itself one way only, among those listed, with its own secret; the code
//! is spent at its first use; the PKCE verifier must be well formed and
//! match; the resource must be the one authorized (`invalid_target`, which
//! spends nothing); a refresh token rotates, and one already rotated, used
//! again, revokes the whole grant. Its settings change what it issues.

mod support;

use mimorya_connectors::testing::{
    CLIENT_SECRET, FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http,
};
use support::oauth::{
    BACK, Proof, VERIFIER, authorized, exchange, renew, token_request, tokens_for,
};

#[tokio::test]
async fn the_code_is_spent_and_pkce_and_the_resource_are_checked() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let url = server.url.as_str();
    let returned = authorized(&authority, url, VERIFIER).await;
    assert_eq!(returned["state"], "etat");
    // The wrong verifier: refused, and the code spent.
    let code = &returned["code"];
    let wrong = "autre-verificateur-pkce-0123456789-abcdefghijklmnop";
    assert_eq!(
        exchange(&authority, code, wrong, url).await["error"],
        "invalid_grant"
    );
    assert_eq!(
        exchange(&authority, code, VERIFIER, url).await["error"],
        "invalid_grant"
    );
    // A verifier RFC 7636 does not allow, though it matches its challenge.
    let returned = authorized(&authority, url, "x").await;
    assert_eq!(
        exchange(&authority, &returned["code"], "x", url).await["error"],
        "invalid_grant"
    );
    // Another resource than the one authorized.
    let returned = authorized(&authority, url, VERIFIER).await;
    let elsewhere = format!("{url}/autre");
    let refused = exchange(&authority, &returned["code"], VERIFIER, &elsewhere).await;
    assert_eq!(refused["error"], "invalid_target");
    let tokens = tokens_for(&authority, url).await;
    let access = tokens["access_token"].as_str().expect("access");
    assert!(authority.accepts(access, url));
    assert!(!authority.accepts(access, &elsewhere));
}

#[tokio::test]
async fn the_client_proves_itself_one_listed_way_only() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let url = server.url.as_str();
    let attempt = async |proof: Proof<'_>| {
        let returned = authorized(&authority, url, VERIFIER).await;
        let pairs = [
            ("grant_type", "authorization_code"),
            ("code", returned["code"].as_str()),
            ("redirect_uri", BACK),
            ("code_verifier", VERIFIER),
            ("resource", url),
        ];
        token_request(&authority, &pairs, proof).await
    };
    // Every way listed: each proof works alone.
    for proof in [
        Proof::Basic(CLIENT_SECRET),
        Proof::Body(CLIENT_SECRET),
        Proof::Public,
    ] {
        assert_eq!(attempt(proof).await.0, 200);
    }
    // Two proofs at once, or a wrong secret: refused; Basic answered 401.
    let (status, answer) = attempt(Proof::BasicAndBody).await;
    assert_eq!(
        (status, answer["error"].as_str()),
        (400, Some("invalid_client"))
    );
    let (status, answer) = attempt(Proof::Basic("autre-secret")).await;
    assert_eq!(
        (status, answer["error"].as_str()),
        (401, Some("invalid_client"))
    );
    assert_eq!(attempt(Proof::Body("autre-secret")).await.0, 400);
    // Only Basic listed: neither a public client nor a secret in the body.
    authority.set(|settings| settings.auth_methods = vec!["client_secret_basic".into()]);
    assert_eq!(attempt(Proof::Public).await.0, 400);
    assert_eq!(attempt(Proof::Body(CLIENT_SECRET)).await.0, 400);
    assert_eq!(attempt(Proof::Basic(CLIENT_SECRET)).await.0, 200);
    let requests = authority.requests();
    assert!(
        requests
            .iter()
            .any(|r| r.endpoint == "token" && r.basic.is_some())
    );
    assert!(
        requests
            .iter()
            .any(|r| r.endpoint == "token" && r.basic.is_none())
    );
}

#[tokio::test]
async fn a_refresh_token_rotates_and_its_reuse_revokes_the_grant() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let url = server.url.as_str();
    let tokens = tokens_for(&authority, url).await;
    let first = tokens["refresh_token"]
        .as_str()
        .expect("refresh")
        .to_string();
    // Another resource: refused, the token not spent.
    let elsewhere = format!("{url}/autre");
    assert_eq!(
        renew(&authority, &first, &elsewhere).await["error"],
        "invalid_target"
    );
    let renewed = renew(&authority, &first, url).await;
    let renewed_access = renewed["access_token"]
        .as_str()
        .expect("renewed")
        .to_string();
    assert!(authority.accepts(&renewed_access, url));
    // The rotated token used again: the whole grant goes.
    assert_eq!(
        renew(&authority, &first, url).await["error"],
        "invalid_grant"
    );
    assert!(!authority.accepts(&renewed_access, url));
    let second = renewed["refresh_token"].as_str().expect("rotated");
    assert_eq!(
        renew(&authority, second, url).await["error"],
        "invalid_grant"
    );
}

#[tokio::test]
async fn its_settings_change_what_it_issues() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let url = server.url.as_str();
    let tokens = tokens_for(&authority, url).await;
    assert_eq!(tokens["expires_in"], 3600);
    assert!(tokens["refresh_token"].is_string());
    authority.set(|settings| {
        settings.lifetime = None;
        settings.refresh = false;
        settings.off_alphabet = true;
        settings.iss_on_return = true;
    });
    let returned = authorized(&authority, url, VERIFIER).await;
    assert_eq!(returned.get("iss"), Some(&authority.issuer));
    let tokens = exchange(&authority, &returned["code"], VERIFIER, url).await;
    assert!(tokens.get("expires_in").is_none() && tokens.get("refresh_token").is_none());
    assert!(
        tokens["access_token"]
            .as_str()
            .expect("access")
            .contains('é')
    );
}
