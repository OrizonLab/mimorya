//! A login whose password check is in flight when a reset, a disabling or a
//! password change commits does not open a session: the session would escape
//! the revocation that change made. Races made deterministic with
//! `support::locks`: the change, then the login, queue on the account row.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::json;
use support::accounts::create_account;
use support::locks::{hold_rows, release_after, wait_for_waiters};
use support::{TestApi, TestResponse};

const NEW_PASSWORD: &str = "une-phrase-de-passe-solide";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_reset_wins_over_a_login_in_flight() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "connexion.reinitialisation").await;
    let reset_url = format!("/admin/users/{}/password", account.id);

    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (reset, login, ()) = tokio::join!(
        paul.post(&reset_url, json!({})),
        async {
            wait_for_waiters(&api, 1).await;
            api.try_login(&account.email, &account.password).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(reset.status, StatusCode::OK);
    assert_refused(&login);
    let temporary = reset.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        api.try_login(&account.email, &temporary).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabling_wins_over_a_login_in_flight() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "connexion.desactivation").await;
    let url = format!("/admin/users/{}", account.id);

    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (disabled, login, ()) = tokio::join!(
        paul.patch(&url, json!({ "status": "disabled" })),
        async {
            wait_for_waiters(&api, 1).await;
            api.try_login(&account.email, &account.password).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(disabled.status, StatusCode::OK);
    // No session was opened, so re-enabling the account revives none.
    assert_refused(&login);
    paul.patch(&url, json!({ "status": "active" })).await;
    assert_eq!(
        api.try_login(&account.email, &account.password)
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_password_change_wins_over_a_login_with_the_old_one() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "connexion.changement").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    let change = json!({ "currentPassword": account.password, "newPassword": NEW_PASSWORD });

    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (changed, login, ()) = tokio::join!(
        session.post("/account/password", change),
        async {
            wait_for_waiters(&api, 1).await;
            api.try_login(&account.email, &account.password).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    assert_refused(&login);
    assert_eq!(session.get("/auth/me").await.status, StatusCode::OK);
}

/// Refused like wrong credentials, without any session cookie.
fn assert_refused(login: &TestResponse) {
    assert_eq!(login.status, StatusCode::UNAUTHORIZED, "{}", login.text());
    assert!(login.headers.get("set-cookie").is_none());
}
