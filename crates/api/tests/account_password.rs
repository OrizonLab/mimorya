//! A user changing their own password: the current one is required (and
//! guessing it is limited like a login), other sessions end, the caller's
//! stays, and a reset, a disabling or another change committed while it runs
//! is never undone (races made deterministic with `support::locks`).

mod support;

use axum::http::StatusCode;
use mimorya_api::auth::credentials::hash_password;
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::accounts::{NewAccount, create_account};
use support::locks::{hold_rows, release_after, wait_for_waiters};

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

/// The change's request body, from the account's provisional password.
fn change_from(account: &NewAccount) -> serde_json::Value {
    json!({ "currentPassword": account.password, "newPassword": NEW_PASSWORD })
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_reset_queued_first_wins_over_the_change() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "course.reinitialisation").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    let reset_url = format!("/admin/users/{}/password", account.id);

    // The reset, then the change, queue on the account row held by the test.
    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (reset, changed, ()) = tokio::join!(
        paul.post(&reset_url, json!({})),
        async {
            wait_for_waiters(&api, 1).await;
            session
                .post("/account/password", change_from(&account))
                .await
        },
        release_after(&api, 2, hold)
    );
    assert_eq!(reset.status, StatusCode::OK);
    // Refused, without deadlock: both take the account row before the sessions.
    assert_eq!(changed.status, StatusCode::CONFLICT, "{}", changed.text());
    let temporary = reset.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        api.try_login(&account.email, &temporary).await.status,
        StatusCode::OK
    );
    assert_eq!(
        api.try_login(&account.email, NEW_PASSWORD).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_change_from_a_session_ended_meanwhile_is_refused() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "course.desactivation").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    let url = format!("/admin/users/{}", account.id);

    // The disabling ends the sessions but keeps the password: only the
    // session check can refuse the change.
    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (disabled, changed, ()) = tokio::join!(
        paul.patch(&url, json!({ "status": "disabled" })),
        async {
            wait_for_waiters(&api, 1).await;
            session
                .post("/account/password", change_from(&account))
                .await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(disabled.status, StatusCode::OK);
    assert_eq!(
        changed.status,
        StatusCode::UNAUTHORIZED,
        "{}",
        changed.text()
    );
    paul.patch(&url, json!({ "status": "active" })).await;
    assert_eq!(
        api.try_login(&account.email, &account.password)
            .await
            .status,
        StatusCode::OK,
        "the password is unchanged"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_change_based_on_a_replaced_password_is_refused() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "course.empreinte").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    for n in 0..9 {
        let guess = json!({ "currentPassword": format!("essai-{n}"), "newPassword": NEW_PASSWORD });
        let wrong = session.post("/account/password", guess).await;
        assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    }

    // Another change commits while this one waits: same password, new salt,
    // no session ended. Only the conditional write can refuse it.
    let mut hold = hold_rows(&api, "users", &[account.id]).await;
    let (changed, ()) = tokio::join!(
        session.post("/account/password", change_from(&account)),
        async {
            wait_for_waiters(&api, 1).await;
            let resalted = hash_password(&account.password).unwrap();
            sqlx::query("update users set password_hash = $2 where id = $1")
                .bind(account.id)
                .bind(resalted)
                .execute(&mut *hold)
                .await
                .unwrap();
            hold.commit().await.unwrap();
        },
    );
    assert_eq!(changed.status, StatusCode::CONFLICT, "{}", changed.text());
    // The refused change knew the current password: it is not counted as the
    // tenth failed guess, so the account can still sign in.
    assert_eq!(
        api.try_login(&account.email, &account.password)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        api.try_login(&account.email, NEW_PASSWORD).await.status,
        StatusCode::UNAUTHORIZED
    );
}
