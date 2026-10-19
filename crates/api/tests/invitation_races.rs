//! Invitations under concurrency (ADR-027): two acceptances of one link, an
//! acceptance while the link is resent, an acceptance while the account is
//! disabled. Each runs in a known order (`support::locks`: the account's row
//! is held until both requests wait for it) and ends in one consistent state.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{INVITED_PASSWORD, accept, directory_entry, invite_account};
use support::locks::{hold_rows, release_after, wait_for_waiters};

const OTHER_PASSWORD: &str = "une-autre-phrase-de-passe";

fn link(invitation: &Value) -> String {
    invitation["link"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_link_activates_its_account_once() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, invitation) = invite_account(&paul, "course.double").await;
    let link = link(&invitation);

    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (first, second, ()) = tokio::join!(
        accept(&api, &link, INVITED_PASSWORD),
        async {
            wait_for_waiters(&api, 1).await;
            accept(&api, &link, OTHER_PASSWORD).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    assert_eq!(second.status, StatusCode::NOT_FOUND);
    api.sign_in_with(&account.email, INVITED_PASSWORD).await;
    let refused = api.try_login(&account.email, OTHER_PASSWORD).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_resent_meanwhile_retires_the_former_one() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, first) = invite_account(&paul, "course.renvoi").await;
    let former_link = link(&first);
    let resend = format!("/admin/users/{}/invitation", account.id);

    // The former link is checked, then waits while the new one is stored.
    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (resent, former, ()) = tokio::join!(
        paul.post(&resend, json!({})),
        async {
            wait_for_waiters(&api, 1).await;
            accept(&api, &former_link, INVITED_PASSWORD).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(resent.status, StatusCode::OK, "{}", resent.text());
    assert_eq!(former.status, StatusCode::NOT_FOUND);
    let current = accept(&api, &link(&resent.json()["invitation"]), OTHER_PASSWORD).await;
    assert_eq!(current.status, StatusCode::OK, "{}", current.text());
    api.sign_in_with(&account.email, OTHER_PASSWORD).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_acceptance_and_a_resend_never_leave_a_dead_link() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, invitation) = invite_account(&paul, "course.acceptation").await;
    let link = link(&invitation);
    let resend = format!("/admin/users/{}/invitation", account.id);

    // Accepted first: the resend then finds an active account.
    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (accepted, resent, ()) = tokio::join!(
        accept(&api, &link, INVITED_PASSWORD),
        async {
            wait_for_waiters(&api, 1).await;
            paul.post(&resend, json!({})).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    assert_eq!(resent.status, StatusCode::CONFLICT);
    let listed = directory_entry(&paul, account.id).await;
    assert_eq!(listed["status"], "active");
    assert_eq!(listed["invitationExpiresAt"], Value::Null);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_disabled_meanwhile_is_not_activated() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, invitation) = invite_account(&paul, "course.desactivation").await;
    let link = link(&invitation);
    let url = format!("/admin/users/{}", account.id);

    // Both lock the account before its invitation: no deadlock.
    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (disabled, accepted, ()) = tokio::join!(
        paul.patch(&url, json!({ "status": "disabled" })),
        async {
            wait_for_waiters(&api, 1).await;
            accept(&api, &link, INVITED_PASSWORD).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(accepted.status, StatusCode::NOT_FOUND);
    assert_eq!(
        directory_entry(&paul, account.id).await["status"],
        "disabled"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_acceptance_then_a_disabling_both_complete() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, invitation) = invite_account(&paul, "course.ordre").await;
    let link = link(&invitation);
    let url = format!("/admin/users/{}", account.id);

    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (accepted, disabled, ()) = tokio::join!(
        accept(&api, &link, INVITED_PASSWORD),
        async {
            wait_for_waiters(&api, 1).await;
            paul.patch(&url, json!({ "status": "disabled" })).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(
        directory_entry(&paul, account.id).await["status"],
        "disabled"
    );
    let refused = api.try_login(&account.email, INVITED_PASSWORD).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_invitation_expiring_meanwhile_is_refused() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, invitation) = invite_account(&paul, "course.echeance").await;
    let link = link(&invitation);

    // Checked while valid, then it expires while the acceptance waits for
    // the account: after the acceptance's transaction began, before its
    // statement that takes the link.
    let hold = hold_rows(&api, "users", &[account.id]).await;
    let (late, ()) = tokio::join!(accept(&api, &link, INVITED_PASSWORD), async {
        wait_for_waiters(&api, 1).await;
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        // Fails rather than hangs if the acceptance ever held the link.
        sqlx::query("set local lock_timeout = '5s'")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("update user_invitations set expires_at = clock_timestamp()")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        hold.commit().await.unwrap();
    });
    assert_eq!(late.status, StatusCode::NOT_FOUND);
    assert_eq!(
        directory_entry(&paul, account.id).await["status"],
        "invited"
    );
}
