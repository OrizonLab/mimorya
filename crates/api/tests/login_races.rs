//! A login racing a reset, a disabling or a password change never keeps a
//! session that change should have ended. Both orders are tested, made
//! deterministic with `support::locks`: the change queued first on the account
//! row (the login is refused), and the login queued first (it opens its
//! session, which the change then ends).

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::accounts::create_account;
use support::locks::{hold_rows, release_after, wait_for_waiters};
use support::{TestApi, TestResponse};
use uuid::Uuid;

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
    assert_eq!(
        denied_login_reason(&api, account.id).await.as_deref(),
        Some("Compte modifié pendant la connexion")
    );
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_login_queued_before_a_reset_loses_its_session() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "connexion.avant.reinitialisation").await;
    let reset_url = format!("/admin/users/{}/password", account.id);

    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (login, reset, ()) = tokio::join!(
        api.try_login(&account.email, &account.password),
        async {
            wait_for_waiters(&api, 1).await;
            paul.post(&reset_url, json!({})).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(login.status, StatusCode::OK, "{}", login.text());
    assert_eq!(reset.status, StatusCode::OK);
    let me = api.session_from(&login).get("/auth/me").await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_login_queued_before_a_disabling_stays_ended_after_re_enabling() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "connexion.avant.desactivation").await;
    let url = format!("/admin/users/{}", account.id);

    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (login, disabled, ()) = tokio::join!(
        api.try_login(&account.email, &account.password),
        async {
            wait_for_waiters(&api, 1).await;
            paul.patch(&url, json!({ "status": "disabled" })).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(login.status, StatusCode::OK, "{}", login.text());
    assert_eq!(disabled.status, StatusCode::OK);
    paul.patch(&url, json!({ "status": "active" })).await;
    let me = api.session_from(&login).get("/auth/me").await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_login_queued_before_a_password_change_loses_its_session() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "connexion.avant.changement").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    let change = json!({ "currentPassword": account.password, "newPassword": NEW_PASSWORD });

    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (login, changed, ()) = tokio::join!(
        api.try_login(&account.email, &account.password),
        async {
            wait_for_waiters(&api, 1).await;
            session.post("/account/password", change).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(login.status, StatusCode::OK, "{}", login.text());
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let me = api.session_from(&login).get("/auth/me").await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    assert_eq!(session.get("/auth/me").await.status, StatusCode::OK);
}

/// Reason of the account's denied login, audited off the response path.
async fn denied_login_reason(api: &TestApi, user_id: Uuid) -> Option<String> {
    for _ in 0..50 {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        let reason: Option<Option<String>> = sqlx::query_scalar(
            "select reason from audit_log \
             where action = 'auth.login' and actor_user_id = $1 and outcome = 'denied'",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
        if let Some(reason) = reason {
            return reason;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

/// Refused like wrong credentials, without any session cookie.
fn assert_refused(login: &TestResponse) {
    assert_eq!(login.status, StatusCode::UNAUTHORIZED, "{}", login.text());
    assert!(login.headers.get("set-cookie").is_none());
}
