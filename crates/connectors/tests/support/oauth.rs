//! An MCP server's OAuth as the tests reach it (ADR-171): plain requests to
//! a server or its documents, a whole authorization with PKCE at the
//! simulated authorization server, and requests to its token endpoint.

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use mimorya_connectors::outbound::http_client;
use mimorya_connectors::testing::{CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer};
use reqwest_mcp::header::{CONTENT_TYPE, LOCATION, WWW_AUTHENTICATE};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::mcp::PRIVATE;

/// Where the authorization sends the browser back.
pub const BACK: &str = "http://127.0.0.1:1/retour";
/// A verifier as RFC 7636 writes one.
pub const VERIFIER: &str = "verificateur-pkce-de-test-0123456789-abcdefghijklmnop";

/// A request to `url`: its status, its challenge and its body.
pub async fn get(url: &str) -> (u16, Option<String>, Vec<u8>) {
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

/// The authorization of `resource` with PKCE and `verifier`: the parameters
/// sent back to `BACK`.
pub async fn authorized(
    authority: &FakeAuthorizationServer,
    resource: &str,
    verifier: &str,
) -> HashMap<String, String> {
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut address = url::Url::parse(&format!("{}/authorize", authority.issuer)).expect("address");
    address.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", BACK),
        ("state", "etat"),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("resource", resource),
    ]);
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

/// How the client proves itself in a request to the token endpoint.
#[derive(Clone, Copy)]
pub enum Proof<'a> {
    Basic(&'a str),
    Body(&'a str),
    BasicAndBody,
    Public,
}

/// Posts `pairs` to the token endpoint, the client proving itself as
/// `proof`: the status and the answer.
pub async fn token_request(
    authority: &FakeAuthorizationServer,
    pairs: &[(&str, &str)],
    proof: Proof<'_>,
) -> (u16, Value) {
    let mut pairs: Vec<(&str, &str)> = pairs.to_vec();
    match proof {
        Proof::Body(secret) => pairs.extend([("client_id", CLIENT_ID), ("client_secret", secret)]),
        Proof::BasicAndBody => pairs.push(("client_secret", CLIENT_SECRET)),
        Proof::Public => pairs.push(("client_id", CLIENT_ID)),
        Proof::Basic(_) => {}
    }
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    let mut request = http_client(PRIVATE)
        .expect("client")
        .post(format!("{}/token", authority.issuer))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body);
    match proof {
        Proof::Basic(secret) => request = request.basic_auth(CLIENT_ID, Some(secret)),
        Proof::BasicAndBody => request = request.basic_auth(CLIENT_ID, Some(CLIENT_SECRET)),
        _ => {}
    }
    let answer = request.send().await.expect("answer");
    let status = answer.status().as_u16();
    (
        status,
        serde_json::from_slice(&answer.bytes().await.expect("body")).expect("json"),
    )
}

/// Exchanges `code` for tokens, the client in the Basic header.
pub async fn exchange(
    authority: &FakeAuthorizationServer,
    code: &str,
    verifier: &str,
    resource: &str,
) -> Value {
    let pairs = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", BACK),
        ("code_verifier", verifier),
        ("resource", resource),
    ];
    token_request(authority, &pairs, Proof::Basic(CLIENT_SECRET))
        .await
        .1
}

/// Renews with `token`, the client in the Basic header.
pub async fn renew(authority: &FakeAuthorizationServer, token: &str, resource: &str) -> Value {
    let pairs = [
        ("grant_type", "refresh_token"),
        ("refresh_token", token),
        ("resource", resource),
    ];
    token_request(authority, &pairs, Proof::Basic(CLIENT_SECRET))
        .await
        .1
}

/// A whole authorization of `resource`: the tokens granted.
pub async fn tokens_for(authority: &FakeAuthorizationServer, resource: &str) -> Value {
    let returned = authorized(authority, resource, VERIFIER).await;
    exchange(authority, &returned["code"], VERIFIER, resource).await
}
