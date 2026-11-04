//! The tokens of an authorization the platform lets go of, revoked at its
//! authorization server (RFC 7009, ADR-172): the refresh token first, then
//! the access token, at the declared revocation endpoint, the client
//! proving itself as declared; the grant is gone there, its refresh token
//! no longer renews. A server naming no revocation endpoint, one that
//! cannot be reached, or one refusing the client, is said, never with a
//! token; a declaration kept before the endpoint was read still reads.

mod support;

use mimorya_connectors::mcp::oauth::{
    HeldToken, NO_REVOCATION_ENDPOINT, OAuthDeclaration, RenewalFailure, refresh_tokens,
    revoke_tokens,
};
use mimorya_connectors::testing::{CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer};
use support::mcp::PRIVATE;
use support::oauth::declared_and_granted;

#[tokio::test]
async fn the_tokens_let_go_of_are_revoked_at_the_authorization_server() {
    let authority = FakeAuthorizationServer::start().await;
    let (declaration, granted, url) = declared_and_granted(&authority, true).await;
    let refresh = granted.refresh_token.expect("renewable");
    let expected = format!("{}/revoke", authority.issuer);
    assert_eq!(
        declaration.revocation_endpoint.as_deref(),
        Some(expected.as_str())
    );
    let held = [
        HeldToken {
            token: &refresh,
            refresh: true,
        },
        HeldToken {
            token: &granted.access_token,
            refresh: false,
        },
    ];
    revoke_tokens(&declaration, &held, Some(CLIENT_SECRET), PRIVATE)
        .await
        .expect("revoked");
    // Asked in order, as the client, never for a resource.
    let asked: Vec<_> = authority
        .requests()
        .into_iter()
        .filter(|asked| asked.endpoint == "revoke")
        .collect();
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[0].params["token"], refresh);
    assert_eq!(asked[0].params["token_type_hint"], "refresh_token");
    assert_eq!(asked[1].params["token_type_hint"], "access_token");
    assert_eq!(asked[0].params["client_secret"], CLIENT_SECRET);
    assert!(!asked[0].params.contains_key("resource"));
    // Gone there: the access token refused, the refresh token renews nothing.
    assert!(!authority.accepts(&granted.access_token, &url));
    let renewed = refresh_tokens(&declaration, &refresh, Some(CLIENT_SECRET), PRIVATE).await;
    assert!(matches!(renewed, Err(RenewalFailure::Refused(_))));
}

#[tokio::test]
async fn what_keeps_a_revocation_from_being_asked_is_said_without_a_token() {
    let authority = FakeAuthorizationServer::start().await;
    let (declaration, granted, url) = declared_and_granted(&authority, false).await;
    let refresh = granted.refresh_token.expect("renewable");
    let held = [HeldToken {
        token: &refresh,
        refresh: true,
    }];
    // A server naming no revocation endpoint.
    let unnamed = OAuthDeclaration {
        revocation_endpoint: None,
        ..declaration.clone()
    };
    let said = revoke_tokens(&unnamed, &held, None, PRIVATE)
        .await
        .unwrap_err();
    assert_eq!(said, NO_REVOCATION_ENDPOINT);
    // One that cannot be reached.
    let away = OAuthDeclaration {
        revocation_endpoint: Some("http://127.0.0.1:1/revoke".into()),
        ..declaration.clone()
    };
    let said = revoke_tokens(&away, &held, None, PRIVATE)
        .await
        .unwrap_err();
    assert!(!said.contains(&refresh), "{said}");
    // One that does not know the client: refused by its code alone.
    let stranger = OAuthDeclaration {
        client_id: "un-autre-client".into(),
        ..declaration.clone()
    };
    let said = revoke_tokens(&stranger, &held, None, PRIVATE)
        .await
        .unwrap_err();
    assert!(
        said.contains("(invalid_client)") && !said.contains(&refresh),
        "{said}"
    );
    // Nothing was revoked meanwhile; the public client then revokes it.
    assert!(authority.accepts(&granted.access_token, &url));
    revoke_tokens(&declaration, &held, None, PRIVATE)
        .await
        .expect("revoked");
    let asked = authority.requests();
    let last = asked.last().expect("asked");
    assert_eq!(last.params["client_id"], CLIENT_ID);
    assert!(!last.params.contains_key("client_secret"));
    assert!(!authority.accepts(&granted.access_token, &url));
}

#[test]
fn a_declaration_kept_before_its_revocation_endpoint_still_reads() {
    let kept = serde_json::json!({
        "issuer": "https://auth.example.com",
        "resource": "https://mcp.example.com/mcp",
        "authorizationEndpoint": "https://auth.example.com/authorize",
        "tokenEndpoint": "https://auth.example.com/token",
        "namesIssuerOnReturn": false,
        "clientId": "mimorya",
        "clientAuth": "none",
        "scopes": [],
    });
    let read: OAuthDeclaration = serde_json::from_value(kept.clone()).expect("read");
    assert_eq!(read.revocation_endpoint, None);
    // Written back as it was.
    assert_eq!(serde_json::to_value(&read).expect("written"), kept);
}
