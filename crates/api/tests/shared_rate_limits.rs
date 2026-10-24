//! The authentication limits shared by every `api` process (ADR-088): a
//! client's sign-ins, an account's failures, an address's reset links and
//! the addresses a client asks about count the same whichever process
//! answers, so that spreading attempts over processes gains nothing. The
//! database keeps a digest of each key only, which the application role
//! cannot read, and the maintenance purges the windows long over; the
//! global limit per request stays each process's own.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use support::limits::{age_windows, windows_of};
use support::resets::ask_reset;
use support::sso::{SSO_DEPLOYMENT, try_discover};
use support::{PASSWORD, TestApi, app_url, email};

/// Two processes of the same deployment: each limit set to `per_minute`.
async fn two_processes(per_minute: &'static str) -> (TestApi, TestApi) {
    let mut env = SSO_DEPLOYMENT.to_vec();
    env.push(("LOGIN_RATE_LIMIT_PER_MINUTE", per_minute));
    (
        TestApi::start_with(&env).await,
        TestApi::start_with(&env).await,
    )
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_client_s_sign_ins_count_the_same_whichever_process_answers() {
    let (one, two) = two_processes("3").await;
    for (api, i) in [(&one, 0), (&two, 1), (&one, 2)] {
        let tried = api
            .try_login(&format!("inconnu{i}@exemple.example"), "x")
            .await;
        assert_eq!(tried.status, StatusCode::UNAUTHORIZED);
    }
    for api in [&one, &two] {
        let refused = api.try_login(&email(user::CLAIRE), PASSWORD).await;
        assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_s_failures_count_together_and_a_success_gives_its_slot_back() {
    let (one, two) = two_processes("1000").await;
    let isabelle = email(user::ISABELLE);
    // A success counts for nothing, wherever it signs in.
    assert_eq!(
        one.try_login(&isabelle, PASSWORD).await.status,
        StatusCode::OK
    );
    for i in 0..10 {
        let api = if i % 2 == 0 { &one } else { &two };
        let failed = api.try_login(&isabelle, "incorrect").await;
        assert_eq!(failed.status, StatusCode::UNAUTHORIZED);
    }
    for api in [&one, &two] {
        let throttled = api.try_login(&isabelle, PASSWORD).await;
        assert_eq!(throttled.status, StatusCode::TOO_MANY_REQUESTS);
    }
    // Another account is counted apart.
    let claire = two.try_login(&email(user::CLAIRE), PASSWORD).await;
    assert_eq!(claire.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_addresses_a_client_asks_about_count_together() {
    let (one, two) = two_processes("1").await;
    let nadia = email(user::NADIA);
    assert_eq!(try_discover(&one, &nadia).await.status, StatusCode::OK);
    // Asked again elsewhere, it costs nothing; another one is refused.
    assert_eq!(try_discover(&two, &nadia).await.status, StatusCode::OK);
    let other = try_discover(&two, "personne@ailleurs.example").await;
    assert_eq!(other.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_address_s_reset_links_count_together() {
    let one = TestApi::start_with_mailer().await;
    let two = TestApi::start_with_mailer().await;
    let claire = email(user::CLAIRE);
    for api in [&one, &two, &one] {
        assert_eq!(ask_reset(api, &claire).await.status, StatusCode::ACCEPTED);
    }
    let limited = ask_reset(&two, &claire).await;
    assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(one.mail.sent().len() + two.mail.sent().len(), 3);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_digest_of_each_key_is_kept_out_of_the_application_s_reach() {
    let (one, _two) = two_processes("1000").await;
    let isabelle = email(user::ISABELLE);
    one.try_login(&isabelle, "incorrect").await;
    let windows = windows_of("failed_logins").await;
    assert_eq!(windows.len(), 1);
    let (digest, hits) = &windows[0];
    assert_eq!((digest.len(), *hits), (32, 1));
    assert!(
        !digest
            .windows(8)
            .any(|w| isabelle.as_bytes().windows(8).any(|e| e == w))
    );

    // The application reaches them through their functions only.
    let app = sqlx::PgPool::connect(app_url()).await.unwrap();
    for table in ["rate_limit_windows", "rate_limit_values"] {
        let read = sqlx::query(&format!("select * from {table}"))
            .fetch_all(&app)
            .await;
        let refused = read
            .expect_err("the application reads no window")
            .to_string();
        assert!(refused.contains("permission denied"), "{refused}");
    }
    app.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_maintenance_purges_the_windows_long_over() {
    let first = TestApi::start().await;
    first.try_login(&email(user::JULIEN), "incorrect").await;
    assert_eq!(windows_of("failed_logins").await.len(), 1);
    age_windows(3).await;

    // A server starting runs the maintenance at once.
    let _second = TestApi::start().await;
    for _ in 0..100 {
        if windows_of("failed_logins").await.is_empty() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("window long over not purged");
}
