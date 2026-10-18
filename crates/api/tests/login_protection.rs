//! Protection of the login route: client addresses that cannot be forged,
//! attempts limited per account (simultaneous ones included), and denied
//! logins audited without their content.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use futures::future::join_all;
use mimorya_core::demo::{tenant, user};
use support::{PASSWORD, TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn ignores_forged_forwarded_addresses_without_a_trusted_proxy() {
    let api = TestApi::start_with(&[("LOGIN_RATE_LIMIT_PER_MINUTE", "3")]).await;
    for i in 0..3 {
        let response = api
            .try_login_from(
                &format!("inconnu{i}@exemple.example"),
                "incorrect",
                Some(&format!("10.0.0.{i}")),
            )
            .await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    }
    // A fresh forged address does not open a new window.
    let forged = api
        .try_login_from("inconnu9@exemple.example", "incorrect", Some("10.9.9.9"))
        .await;
    assert_eq!(forged.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn throttles_failed_logins_per_account_whatever_the_address() {
    // Even with forwarded addresses trusted, failures are counted per account.
    let api = TestApi::start_with(&[("TRUST_PROXY", "true")]).await;
    let isabelle = email(user::ISABELLE);
    for i in 0..10 {
        let response = api
            .try_login_from(&isabelle, "incorrect", Some(&format!("10.1.0.{i}")))
            .await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    }
    let throttled = api
        .try_login_from(&isabelle, PASSWORD, Some("10.1.1.1"))
        .await;
    assert_eq!(throttled.status, StatusCode::TOO_MANY_REQUESTS);
    // Unknown accounts are treated the same way, and other accounts still sign in.
    for _ in 0..10 {
        api.try_login("personne@inconnu.example", "incorrect").await;
    }
    let unknown = api.try_login("personne@inconnu.example", "incorrect").await;
    assert_eq!(unknown.status, StatusCode::TOO_MANY_REQUESTS);
    let claire = api.try_login(&email(user::CLAIRE), PASSWORD).await;
    assert_eq!(claire.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn counts_simultaneous_attempts_against_the_account() {
    let api = TestApi::start_with(&[("TRUST_PROXY", "true")]).await;
    let nadia = email(user::NADIA);
    let addresses: Vec<String> = (0..30).map(|i| format!("10.2.0.{i}")).collect();
    let attempts = addresses
        .iter()
        .map(|address| api.try_login_from(&nadia, "incorrect", Some(address)));
    let statuses: Vec<StatusCode> = join_all(attempts)
        .await
        .into_iter()
        .map(|r| r.status)
        .collect();
    let verified = statuses
        .iter()
        .filter(|s| **s == StatusCode::UNAUTHORIZED)
        .count();
    assert_eq!(verified, 10, "{statuses:?}");
    assert!(
        statuses
            .iter()
            .all(|s| *s == StatusCode::UNAUTHORIZED || *s == StatusCode::TOO_MANY_REQUESTS)
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn audits_denied_logins_without_their_content() {
    let api = TestApi::start().await;
    let response = api.try_login(&email(user::EMMA), "mauvais-secret").await;
    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    // The entry is written off the response path: wait for it, within bounds.
    let mut entry = None;
    for _ in 0..50 {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        entry = sqlx::query_as::<_, (String, Option<String>, serde_json::Value)>(
            "select outcome, reason, metadata from audit_log \
             where action = 'auth.login' and actor_user_id = $1 and outcome = 'denied'",
        )
        .bind(user::EMMA)
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
        if entry.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (outcome, reason, metadata) = entry.expect("denied login audited");
    assert_eq!(outcome, "denied");
    assert_eq!(reason.as_deref(), Some("Mot de passe invalide"));
    assert!(!metadata.to_string().contains("mauvais-secret"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn counts_only_failures_against_the_account() {
    let api = TestApi::start().await;
    let julien = email(user::JULIEN);
    for _ in 0..9 {
        let failure = api.try_login(&julien, "incorrect").await;
        assert_eq!(failure.status, StatusCode::UNAUTHORIZED);
    }
    // A successful login gives its attempt back…
    assert_eq!(
        api.try_login(&julien, PASSWORD).await.status,
        StatusCode::OK
    );
    // …so the tenth failure is still verified, and the next one is throttled.
    let tenth = api.try_login(&julien, "incorrect").await;
    assert_eq!(tenth.status, StatusCode::UNAUTHORIZED);
    let eleventh = api.try_login(&julien, "incorrect").await;
    assert_eq!(eleventh.status, StatusCode::TOO_MANY_REQUESTS);
}
