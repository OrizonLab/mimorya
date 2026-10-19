//! Every API response carries the hardening headers, refusals included: its
//! JSON is never rendered, framed nor sniffed, and no cache keeps it unless
//! the route chose its own policy (the event stream).

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::json;
use support::{TestApi, TestResponse};

fn assert_hardened(response: &TestResponse, cache: &str) {
    let header = |name: &str| {
        response.headers[name]
            .to_str()
            .unwrap_or_else(|_| panic!("{name}"))
            .to_string()
    };
    assert_eq!(
        header("content-security-policy"),
        "default-src 'none'; frame-ancestors 'none'"
    );
    assert_eq!(header("x-content-type-options"), "nosniff");
    assert_eq!(header("x-frame-options"), "DENY");
    assert_eq!(header("referrer-policy"), "no-referrer");
    assert_eq!(header("cache-control"), cache);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn every_response_is_hardened_refusals_included() {
    let api = TestApi::start_with(&[("LOGIN_RATE_LIMIT_PER_MINUTE", "1")]).await;
    let paul = api.sign_in(user::PAUL).await;
    let me = paul.get("/auth/me").await;
    assert_eq!(me.status, StatusCode::OK);
    let anonymous = api.anonymous();
    let refusals = [
        (anonymous.get("/auth/me").await, StatusCode::UNAUTHORIZED),
        (paul.get("/nulle-part").await, StatusCode::NOT_FOUND),
        // The second login of this client is over its limit.
        (
            api.try_login("personne@nulle-part.example", "mauvais")
                .await,
            StatusCode::TOO_MANY_REQUESTS,
        ),
    ];
    for response in [&me, &api.anonymous().get("/health").await] {
        assert_hardened(response, "no-store");
    }
    for (response, status) in &refusals {
        assert_eq!(response.status, *status, "{}", response.text());
        assert_hardened(response, "no-store");
    }
    // A request refused by the CSRF protection, before any route.
    let forged = api
        .send(
            axum::http::Request::post("/auth/logout")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(forged.status, StatusCode::FORBIDDEN, "{}", forged.text());
    assert_hardened(&forged, "no-store");
}
