//! An MCP server protected by OAuth (ADR-171): it says so when refused, it
//! opens to a token issued for its very address and to no other, and its
//! challenge and metadata lead to its authorization server only when they
//! come from its own origin and name it; the simulated authorization server
//! the later lots rely on checks PKCE S256, the resource and the client,
//! and revokes a grant whose rotated refresh token is used again.

mod support;

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use mimorya_connectors::mcp::OAUTH_REQUIRED;
use mimorya_connectors::mcp::oauth::{
    ClientAuth, bearer_challenge, metadata_locations, protected_resource_locations,
    read_protected_resource, read_server_metadata, same_origin,
};
use mimorya_connectors::outbound::{ACCESS_DENIED, http_client};
use mimorya_connectors::testing::{
    CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http,
};
use reqwest_mcp::header::{CONTENT_TYPE, LOCATION, WWW_AUTHENTICATE};
use sha2::{Digest, Sha256};
use support::mcp::{PRIVATE, http, start_mcp};

/// `base` with `pairs` as its query.
fn with_query(base: &str, pairs: &[(&str, &str)]) -> String {
    let mut url = url::Url::parse(base).expect("address");
    url.query_pairs_mut().extend_pairs(pairs);
    url.to_string()
}

/// `pairs` as a form body.
fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

/// Posts the form `pairs` to the token endpoint of `authority`.
async fn token_request(
    authority: &FakeAuthorizationServer,
    pairs: &[(&str, &str)],
    basic: bool,
) -> serde_json::Value {
    let mut request = http_client(PRIVATE)
        .expect("client")
        .post(format!("{}/token", authority.issuer))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form(pairs));
    if basic {
        request = request.basic_auth(CLIENT_ID, Some(CLIENT_SECRET));
    }
    let answer = request.send().await.expect("answer");
    serde_json::from_slice(&answer.bytes().await.expect("body")).expect("json")
}

/// The authorization of `resource`, with PKCE: the code sent back.
async fn authorized_code(
    authority: &FakeAuthorizationServer,
    resource: &str,
    verifier: &str,
) -> HashMap<String, String> {
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let address = with_query(
        &format!("{}/authorize", authority.issuer),
        &[
            ("response_type", "code"),
            ("client_id", CLIENT_ID),
            ("redirect_uri", BACK),
            ("state", "etat"),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("resource", resource),
        ],
    );
    let answer = http_client(PRIVATE)
        .expect("client")
        .get(address)
        .send()
        .await
        .expect("answer");
    let location = answer.headers()[LOCATION]
        .to_str()
        .expect("location")
        .to_string();
    url::Url::parse(&location)
        .expect("return")
        .query_pairs()
        .into_owned()
        .collect()
}

const BACK: &str = "http://127.0.0.1:1/retour";

async fn failure(url: &str, token: Option<&str>) -> String {
    let error = start_mcp(http(url), token, PRIVATE)
        .await
        .err()
        .expect("refused");
    format!("{error:#}")
}

async fn get(url: &str) -> (u16, Option<String>, Vec<u8>) {
    let response = http_client(PRIVATE)
        .expect("client")
        .get(url)
        .send()
        .await
        .expect("answer");
    let status = response.status().as_u16();
    let challenge = response
        .headers()
        .get(WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    (
        status,
        challenge,
        response.bytes().await.expect("body").to_vec(),
    )
}

#[tokio::test]
async fn a_protected_server_says_it_wants_an_oauth_authorization() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    assert!(failure(&server.url, None).await.contains(OAUTH_REQUIRED));
    // A token typed by hand will not do.
    let typed = failure(&server.url, Some("jeton-saisi-a-la-main-0123")).await;
    assert!(typed.contains(OAUTH_REQUIRED), "{typed}");
    // A refusal naming no metadata is an ordinary one.
    let plain = serve_oauth_mcp_http(&authority, OAuthQuirk::NoMetadata).await;
    let refused = failure(&plain.url, None).await;
    assert!(
        refused.contains(ACCESS_DENIED) && !refused.contains(OAUTH_REQUIRED),
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
    assert!(
        server
            .authorizations()
            .contains(&Some(format!("Bearer {token}")))
    );
    // Issued for the other server: refused by this one.
    let elsewhere = authority.issue(&other.url);
    assert!(
        failure(&server.url, Some(&elsewhere))
            .await
            .contains(OAUTH_REQUIRED)
    );
    // Revoked at the authorization server: refused.
    authority.revoke_all();
    assert!(
        failure(&server.url, Some(&token))
            .await
            .contains(OAUTH_REQUIRED)
    );
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
    let (_, _, body) = get(&metadata).await;
    let resource = read_protected_resource(&body, &server.url, PRIVATE).expect("metadata");
    assert_eq!(
        resource.authorization_servers,
        std::slice::from_ref(&authority.issuer)
    );
    let issuer = &resource.authorization_servers[0];
    let (status, _, body) = get(&metadata_locations(issuer)[0]).await;
    assert_eq!(status, 200);
    let found = read_server_metadata(&body, issuer, PRIVATE).expect("authorization server");
    assert_eq!(found.token_endpoint, format!("{issuer}/token"));
    assert_eq!(found.client_auth(true), Ok(ClientAuth::ClientSecretPost));
    assert_eq!(found.client_auth(false), Ok(ClientAuth::None));
    assert!(found.offers_offline_access());
    // Without the private network the operator allows, refused.
    assert!(read_protected_resource(&body, &server.url, Default::default()).is_err());
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
    let (_, _, body) = get(&protected_resource_locations(&other.url)[0]).await;
    assert!(read_protected_resource(&body, &other.url, PRIVATE).is_err());
    // No metadata at all, nor named.
    let none = serve_oauth_mcp_http(&authority, OAuthQuirk::NoMetadata).await;
    let (_, header, _) = get(&none.url).await;
    assert_eq!(
        bearer_challenge(&header.expect("challenge")).and_then(|c| c.resource_metadata),
        None
    );
    assert_eq!(
        get(&protected_resource_locations(&none.url)[0]).await.0,
        404
    );
    // An authorization server naming another issuer.
    authority.set(|settings| settings.wrong_issuer = true);
    let (_, _, body) = get(&metadata_locations(&authority.issuer)[0]).await;
    assert!(read_server_metadata(&body, &authority.issuer, PRIVATE).is_err());
}

/// Exchanges `code`, the client in the Basic header.
async fn exchange(
    authority: &FakeAuthorizationServer,
    code: &str,
    verifier: &str,
    resource: &str,
) -> serde_json::Value {
    let pairs = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", BACK),
        ("code_verifier", verifier),
        ("resource", resource),
    ];
    token_request(authority, &pairs, true).await
}

/// Renews with `token`, the client in the Basic header.
async fn renew(
    authority: &FakeAuthorizationServer,
    token: &str,
    resource: &str,
) -> serde_json::Value {
    let pairs = [
        ("grant_type", "refresh_token"),
        ("refresh_token", token),
        ("resource", resource),
    ];
    token_request(authority, &pairs, true).await
}

#[tokio::test]
async fn the_authorization_server_checks_pkce_the_resource_and_rotation() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let url = server.url.as_str();
    let verifier = "verificateur-pkce-de-test-0123456789-abcdefghijklmnop";
    let returned = authorized_code(&authority, url, verifier).await;
    assert_eq!(returned["state"], "etat");
    // The wrong verifier: refused, and the code spent.
    let code = &returned["code"];
    assert_eq!(
        exchange(&authority, code, "autre-verificateur", url).await["error"],
        "invalid_grant"
    );
    assert_eq!(
        exchange(&authority, code, verifier, url).await["error"],
        "invalid_grant"
    );
    // Another resource than the one authorized: refused.
    let returned = authorized_code(&authority, url, verifier).await;
    let elsewhere = format!("{url}/autre");
    let refused = exchange(&authority, &returned["code"], verifier, &elsewhere).await;
    assert_eq!(refused["error"], "invalid_grant");
    // The client in the body, as a post: granted.
    let returned = authorized_code(&authority, url, verifier).await;
    let pairs = [
        ("grant_type", "authorization_code"),
        ("code", returned["code"].as_str()),
        ("redirect_uri", BACK),
        ("code_verifier", verifier),
        ("resource", url),
        ("client_id", CLIENT_ID),
        ("client_secret", CLIENT_SECRET),
    ];
    let tokens = token_request(&authority, &pairs, false).await;
    let access = tokens["access_token"].as_str().expect("access");
    assert!(authority.accepts(access, url));
    // Renewed: the refresh token rotates; the old one used again revokes all.
    let first = tokens["refresh_token"]
        .as_str()
        .expect("refresh")
        .to_string();
    let renewed = renew(&authority, &first, url).await;
    let renewed_access = renewed["access_token"]
        .as_str()
        .expect("renewed")
        .to_string();
    assert!(authority.accepts(&renewed_access, url));
    assert_eq!(
        renew(&authority, &first, url).await["error"],
        "invalid_grant"
    );
    assert!(!authority.accepts(&renewed_access, url));
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
