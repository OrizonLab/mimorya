//! What guards an MCP server's authorization return (ADR-171) besides its
//! attempt: past the limit it shares with the start, a return is turned
//! away before the authorization server is asked, its reason audited; a
//! declaration changed while the code was being exchanged keeps nothing,
//! the tokens granted under the former one revoked under it (ADR-172); so
//! does one that comes back after each person became who authorizes the
//! server (ADR-174).

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::http::StatusCode;
use mimorya_connectors::testing::{CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer};
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::connector_oauth::{
    FAILED, api_path, come_back, declared, kept_tokens, refusals_by, refused_because, start,
    try_start,
};
use support::connector_sessions::{alter, token_endpoint};
use support::mcp::PRIVATE_DEPLOYMENT;
use support::sso::location;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Notify;

/// What a held token endpoint grants once let go: tokens the platform
/// could keep.
const GRANTED: &str = r#"{"access_token":"jeton-retenu","token_type":"Bearer",
"expires_in":3600,"refresh_token":"renouvellement-retenu"}"#;

/// A token endpoint that reads the request, then answers only once let go:
/// its address, how many requests it read, and what lets it go.
async fn held() -> (String, Arc<AtomicUsize>, Arc<Notify>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("port");
    let url = format!("http://{}/token", listener.local_addr().expect("address"));
    let (read, release) = (Arc::new(AtomicUsize::new(0)), Arc::new(Notify::new()));
    let (counted, gate) = (read.clone(), release.clone());
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = Vec::new();
            let mut chunk = [0u8; 4096];
            // Up to its form, which names the grant.
            while !request.windows(10).any(|w| w == b"grant_type") {
                match socket.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                }
            }
            counted.fetch_add(1, Ordering::SeqCst);
            gate.notified().await;
            let answer = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{GRANTED}",
                GRANTED.len()
            );
            let _ = socket.write_all(answer.as_bytes()).await;
        }
    });
    (url, read, release)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_return_past_the_limit_is_turned_away_before_the_server() {
    // One sign-in a minute: the start and the return share two.
    let mut env = PRIVATE_DEPLOYMENT.to_vec();
    env.push(("LOGIN_RATE_LIMIT_PER_MINUTE", "1"));
    let api = TestApi::start_with(&env).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "limite").await;
    let (url, cookie) = start(&paul, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    assert_eq!(try_start(&paul, &id).await.status, StatusCode::OK);
    // The third: turned away, audited, its code never exchanged.
    let turned = come_back(&paul, &back, Some(&cookie)).await;
    assert_eq!(location(&turned), FAILED);
    let reasons = refusals_by(&api, user::PAUL).await;
    assert_eq!(
        reasons.last().map(String::as_str),
        Some("Trop de tentatives")
    );
    assert!(authority.requests().iter().all(|r| r.endpoint != "token"));
    assert!(kept_tokens(&api, &id).await.is_none());
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_return_after_each_person_became_the_holder_keeps_nothing() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "devenu").await;
    let (url, cookie) = start(&paul, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    // Each person authorizes it now (ADR-174): the company's attempt ends.
    let holder = format!("/admin/connectors/{id}/oauth/holder");
    let each = paul.put(&holder, json!({ "perPerson": true })).await;
    assert_eq!(each.status, StatusCode::OK, "{}", each.text());
    assert_eq!(
        location(&come_back(&paul, &back, Some(&cookie)).await),
        FAILED
    );
    assert!(kept_tokens(&api, &id).await.is_none());
    let reason = refused_because(&api, &id).await;
    assert!(reason.contains("changée"), "{reason}");
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_declaration_changed_during_the_exchange_keeps_nothing() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "course").await;
    let (endpoint, read, release) = held().await;
    alter(&api, &id, &token_endpoint(&endpoint)).await;
    let (url, cookie) = start(&paul, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    let returned = come_back(&paul, &back, Some(&cookie));
    // Declared again, as a public client, while the code is exchanged.
    let changed = async {
        while read.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let oauth = format!("/admin/connectors/{id}/oauth");
        let again = paul.put(&oauth, json!({ "clientId": CLIENT_ID })).await;
        assert_eq!(again.status, StatusCode::OK, "{}", again.text());
        release.notify_one();
    };
    let (returned, ()) = tokio::join!(returned, changed);
    assert_eq!(location(&returned), FAILED);
    let reason = refused_because(&api, &id).await;
    assert!(
        reason.contains("changée pendant l’autorisation"),
        "{reason}"
    );
    assert!(kept_tokens(&api, &id).await.is_none());
    // The tokens granted, kept nowhere, revoked under the former
    // declaration, with its secret, by a task of their own.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let revoked = loop {
        let requests = authority.requests();
        let revoked: Vec<_> = requests
            .into_iter()
            .filter(|r| r.endpoint == "revoke")
            .collect();
        if revoked.len() >= 2 || std::time::Instant::now() > deadline {
            break revoked;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let tokens: Vec<_> = revoked.iter().map(|r| r.params["token"].as_str()).collect();
    assert_eq!(tokens, ["renouvellement-retenu", "jeton-retenu"]);
    assert_eq!(revoked[0].params["client_secret"], CLIENT_SECRET);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
