//! The authentication limits shared by every `api` process (ADR-088): a
//! client's sign-ins, an account's failures, an address's reset links and
//! the addresses a client asks about count the same whichever process
//! answers, so that spreading attempts over processes gains nothing. The
//! database keeps a digest of each key and value only, which the application
//! role cannot read: with the limit's name, its HMAC under the key derived
//! from the master key or, without one, its SHA-256 digest (ADR-152); and the
//! maintenance purges the windows long over. The global limit per request
//! stays each process's own. A count that cannot be read lets nobody in.

mod support;

use axum::http::StatusCode;
use mimorya_api::plugins::shared_limit::KeyDigest;
use mimorya_api::secrets::SecretCipher;
use mimorya_core::demo::user;
use sha2::{Digest, Sha256};
use support::identity_provider::{MockIdentityProvider, Person};
use support::limits::{age_windows, allow_counting, values_of, windows_of};
use support::llm::MASTER_KEY;
use support::resets::ask_reset;
use support::sso::{
    SSO_DEPLOYMENT, assert_refused, come_back, declaration, declare, remove, start, try_discover,
};
use support::{PASSWORD, TestApi, app_url, email};

/// The unkeyed digest of `value` for the limit `limiter`: its name, a zero
/// byte and the value.
fn unkeyed(limiter: &str, value: &str) -> Vec<u8> {
    let framed = [limiter.as_bytes(), &[0], value.as_bytes()].concat();
    Sha256::digest(&framed).to_vec()
}

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
    // Its HMAC under the key derived from the deployment's master key, not
    // the digest anyone could compute from a guessed address.
    let cipher = SecretCipher::from_base64(MASTER_KEY).unwrap();
    let keyed = KeyDigest::of(Some(&cipher)).unwrap();
    assert_eq!(digest.as_slice(), keyed.digest("failed_logins", &isabelle));
    assert_ne!(digest.as_slice(), unkeyed("failed_logins", &isabelle));
    // A value too: an address asked about.
    let asked = "quelquun@exemple.example";
    try_discover(&one, asked).await;
    let values = values_of("sso_addresses").await;
    let expected = keyed.digest("sso_addresses", asked).to_vec();
    assert!(values.contains(&expected), "{values:?}");

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
async fn without_a_master_key_a_key_is_kept_as_its_unkeyed_digest() {
    let api = TestApi::start().await;
    let julien = email(user::JULIEN);
    api.try_login(&julien, "incorrect").await;
    let windows = windows_of("failed_logins").await;
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].0.as_slice(), unkeyed("failed_logins", &julien));
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_count_that_cannot_be_read_lets_nobody_in() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let nadia = email(user::NADIA);
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    let (url, cookie) = start(&api, &nadia).await;
    let requests = idp.requests();

    allow_counting(false).await;
    let back = come_back(&api, &idp.authorize(&url), Some(&cookie)).await;
    let login = api.try_login(&email(user::CLAIRE), PASSWORD).await;
    allow_counting(true).await;

    // The return is turned away before the provider; a login fails.
    assert_refused(&back);
    assert_eq!(idp.requests(), requests);
    assert_eq!(idp.exchanges(), 0);
    assert_eq!(login.status, StatusCode::INTERNAL_SERVER_ERROR);
    remove(&paul, &id).await;
}
