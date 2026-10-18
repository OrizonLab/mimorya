//! A user changing their own password: the current one is required (and
//! guessing it is limited like a login), other sessions end, the caller's
//! stays.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::accounts::create_account;

const NEW_PASSWORD: &str = "une-phrase-de-passe-solide";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn replaces_the_password_and_ends_the_other_sessions() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "nouveau.mot.de.passe").await;
    let laptop = api.sign_in_with(&account.email, &account.password).await;
    let phone = api.sign_in_with(&account.email, &account.password).await;

    let wrong = laptop
        .post(
            "/account/password",
            json!({ "currentPassword": "incorrect", "newPassword": NEW_PASSWORD }),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    let same = laptop
        .post(
            "/account/password",
            json!({ "currentPassword": account.password, "newPassword": account.password }),
        )
        .await;
    assert_eq!(same.status, StatusCode::BAD_REQUEST);
    let changed = laptop
        .post(
            "/account/password",
            json!({ "currentPassword": account.password, "newPassword": NEW_PASSWORD }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());

    assert_eq!(laptop.get("/auth/me").await.status, StatusCode::OK);
    assert_eq!(phone.get("/auth/me").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        api.try_login(&account.email, &account.password)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.try_login(&account.email, NEW_PASSWORD).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn limits_guesses_of_the_current_password() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "mot.de.passe.devine").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    let guess =
        |n: usize| json!({ "currentPassword": format!("essai-{n}"), "newPassword": NEW_PASSWORD });
    for n in 0..10 {
        let attempt = session.post("/account/password", guess(n)).await;
        assert_eq!(attempt.status, StatusCode::FORBIDDEN, "attempt {n}");
    }
    let blocked = session.post("/account/password", guess(10)).await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(blocked.json()["error"]["code"], "rate_limited");
    // Logging in to the account is limited by the same counter.
    assert_eq!(
        api.try_login(&account.email, &account.password)
            .await
            .status,
        StatusCode::TOO_MANY_REQUESTS
    );
}
