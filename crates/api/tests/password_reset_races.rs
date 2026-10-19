//! Reset links under concurrency (ADR-028): two uses of one link, a use while
//! a newer link is issued, while an administrator resets the password, while
//! the link expires, and a link asked for while the account is disabled. Each runs in a known order
//! (`support::locks`: the account's row is held until the requests wait for
//! it) and ends in one consistent state.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::locks::{hold_rows, release_after, wait_for_waiters};
use support::resets::{RESET_PASSWORD, complete_reset, new_reset_token};
use support::{PASSWORD, TestApi, email};

const OTHER_PASSWORD: &str = "une-autre-phrase-de-passe";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_link_resets_once() {
    let api = TestApi::start_with_mailer().await;
    let address = email(user::CLAIRE);
    let token = new_reset_token(&api, &address).await;

    let hold = hold_rows(&api, "users", &[user::CLAIRE]).await;
    let (first, second, ()) = tokio::join!(
        complete_reset(&api, &token, RESET_PASSWORD),
        async {
            wait_for_waiters(&api, 1).await;
            complete_reset(&api, &token, OTHER_PASSWORD).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    assert_eq!(second.status, StatusCode::NOT_FOUND);
    api.sign_in_with(&address, RESET_PASSWORD).await;
    let refused = api.try_login(&address, OTHER_PASSWORD).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_replaced_meanwhile_is_refused() {
    let api = TestApi::start_with_mailer().await;
    let address = email(user::NADIA);
    let former = new_reset_token(&api, &address).await;

    // The newer link is stored first (in the background), then the former
    // one, already checked, is used.
    let anonymous = api.anonymous();
    let hold = hold_rows(&api, "users", &[user::NADIA]).await;
    let (asked, late, ()) = tokio::join!(
        anonymous.post("/auth/password-reset", json!({ "email": address })),
        async {
            wait_for_waiters(&api, 1).await;
            complete_reset(&api, &former, RESET_PASSWORD).await
        },
        release_after(&api, 2, hold),
    );
    api.idle().await;
    assert_eq!(asked.status, StatusCode::ACCEPTED);
    assert_eq!(late.status, StatusCode::NOT_FOUND);
    let newer = support::resets::last_reset_token(&api, &address);
    assert_ne!(newer, former);
    let used = complete_reset(&api, &newer, OTHER_PASSWORD).await;
    assert_eq!(used.status, StatusCode::OK, "{}", used.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_s_reset_meanwhile_retires_the_link() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let address = email(user::MARC);
    let token = new_reset_token(&api, &address).await;
    let reset_url = format!("/admin/users/{}/password", user::MARC);

    // The link is checked, then waits while the password is reset.
    let hold = hold_rows(&api, "users", &[user::MARC]).await;
    let (reset, late, ()) = tokio::join!(
        paul.post(&reset_url, json!({})),
        async {
            wait_for_waiters(&api, 1).await;
            complete_reset(&api, &token, RESET_PASSWORD).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    assert_eq!(late.status, StatusCode::NOT_FOUND);
    let temporary = reset.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    api.sign_in_with(&address, &temporary).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_expiring_meanwhile_is_refused() {
    let api = TestApi::start_with_mailer().await;
    let address = email(user::JULIEN);
    let token = new_reset_token(&api, &address).await;

    // Checked while valid, then it expires while the use waits for the
    // account: after the use's transaction began, before its statement that
    // takes the link.
    let hold = hold_rows(&api, "users", &[user::JULIEN]).await;
    let (late, ()) = tokio::join!(complete_reset(&api, &token, RESET_PASSWORD), async {
        wait_for_waiters(&api, 1).await;
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        // Fails rather than hangs if the use ever held the link.
        sqlx::query("set local lock_timeout = '5s'")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("update password_resets set expires_at = clock_timestamp()")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        hold.commit().await.unwrap();
    });
    assert_eq!(late.status, StatusCode::NOT_FOUND);
    api.sign_in_with(&address, PASSWORD).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_disabled_while_its_link_is_issued_gets_none() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let address = email(user::LEA);
    let url = format!("/admin/users/{}", user::LEA);

    // The disabling first, then the link, issued in the background, finds
    // a disabled account.
    let hold = hold_rows(&api, "users", &[user::LEA]).await;
    let (disabled, asked, ()) = tokio::join!(
        paul.patch(&url, json!({ "status": "disabled" })),
        async {
            wait_for_waiters(&api, 1).await;
            api.anonymous()
                .post("/auth/password-reset", json!({ "email": address }))
                .await
        },
        release_after(&api, 2, hold),
    );
    api.idle().await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(asked.status, StatusCode::ACCEPTED);
    assert!(api.mail.sent().is_empty());
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let stored: i64 = sqlx::query_scalar("select count(*) from password_resets where user_id = $1")
        .bind(user::LEA)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(stored, 0);
}
