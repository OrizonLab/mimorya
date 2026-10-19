//! The life of a reset link (ADR-028): a newer link replaces the former one;
//! links expire and the periodic maintenance purges them; any other change
//! of the password or of the account's status retires the link, a profile
//! change does not; asking is limited per address and per client.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::password_resets::purge_expired_password_resets;
use serde_json::json;
use support::accounts::OWN_PASSWORD;
use support::resets::{RESET_PASSWORD, ask_reset, complete_reset, inspect_reset, new_reset_token};
use support::{PASSWORD, TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_newer_link_replaces_the_former_one_and_links_expire() {
    let api = TestApi::start_with_mailer().await;
    let address = email(user::NADIA);
    let first = new_reset_token(&api, &address).await;
    let second = new_reset_token(&api, &address).await;
    assert_ne!(first, second);
    assert_eq!(
        inspect_reset(&api, &first).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(inspect_reset(&api, &second).await.status, StatusCode::OK);

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update password_resets set expires_at = now() - interval '1 second'")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        inspect_reset(&api, &second).await.status,
        StatusCode::NOT_FOUND
    );
    let late = complete_reset(&api, &second, RESET_PASSWORD).await;
    assert_eq!(late.status, StatusCode::NOT_FOUND);
    api.sign_in_with(&address, PASSWORD).await;

    // The periodic maintenance deletes expired links only.
    let current = new_reset_token(&api, &email(user::HUGO)).await;
    let purged = purge_expired_password_resets(api.app.services.db.pool())
        .await
        .unwrap();
    assert!(purged >= 1);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let left: Vec<uuid::Uuid> =
        sqlx::query_scalar("select user_id from password_resets where user_id = any($1)")
            .bind([user::NADIA, user::HUGO])
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    drop(tx);
    assert_eq!(left, [user::HUGO]);
    assert_eq!(inspect_reset(&api, &current).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_periodic_maintenance_purges_expired_links() {
    let first = TestApi::start_with_mailer().await;
    new_reset_token(&first, &email(user::JULIEN)).await;
    let mut tx = first.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update password_resets set expires_at = now() where user_id = $1")
        .bind(user::JULIEN)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // A server starting runs the maintenance at once.
    let second = TestApi::start().await;
    for _ in 0..100 {
        let mut tx = second.app.services.db.begin(tenant::ID).await.unwrap();
        let left: i64 =
            sqlx::query_scalar("select count(*) from password_resets where user_id = $1")
                .bind(user::JULIEN)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        drop(tx);
        if left == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("expired link not purged");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn changing_the_password_or_the_status_otherwise_retires_the_link() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let address = email(user::EMMA);
    let account = format!("/admin/users/{}", user::EMMA);

    // A profile change leaves the link working.
    let token = new_reset_token(&api, &address).await;
    let renamed = paul
        .patch(&account, json!({ "displayName": "Emma Durand-Petit" }))
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.text());
    assert_eq!(inspect_reset(&api, &token).await.status, StatusCode::OK);

    // An administrator's reset retires it.
    let reset = paul.post(&format!("{account}/password"), json!({})).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    let temporary = reset.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        inspect_reset(&api, &token).await.status,
        StatusCode::NOT_FOUND
    );

    // So does the person's own change.
    let token = new_reset_token(&api, &address).await;
    let session = api.sign_in_with(&address, &temporary).await;
    let changed = session
        .post(
            "/account/password",
            json!({ "currentPassword": temporary, "newPassword": OWN_PASSWORD }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    assert_eq!(
        complete_reset(&api, &token, RESET_PASSWORD).await.status,
        StatusCode::NOT_FOUND
    );

    // And a disabling, for good: enabled again, the link is gone.
    let token = new_reset_token(&api, &address).await;
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK);
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK);
    assert_eq!(
        complete_reset(&api, &token, RESET_PASSWORD).await.status,
        StatusCode::NOT_FOUND
    );
    api.sign_in_with(&address, OWN_PASSWORD).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn asking_is_limited_per_address_known_or_not() {
    let api = TestApi::start_with_mailer().await;
    for address in [email(user::CLAIRE), "inconnu@nulle-part.example".into()] {
        for _ in 0..3 {
            assert_eq!(ask_reset(&api, &address).await.status, StatusCode::ACCEPTED);
        }
        let limited = ask_reset(&api, &address.to_uppercase()).await;
        assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS);
    }
    assert_eq!(api.mail.sent().len(), 3);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn every_reset_route_is_limited_per_client() {
    let api = TestApi::start_with_mailer_and(&[("LOGIN_RATE_LIMIT_PER_MINUTE", "3")]).await;
    let statuses = [
        ask_reset(&api, &email(user::CLAIRE)).await.status,
        inspect_reset(&api, "inconnu").await.status,
        complete_reset(&api, "inconnu", RESET_PASSWORD).await.status,
        ask_reset(&api, &email(user::MARC)).await.status,
        inspect_reset(&api, "inconnu").await.status,
        complete_reset(&api, "inconnu", RESET_PASSWORD).await.status,
    ];
    assert_eq!(
        statuses,
        [
            StatusCode::ACCEPTED,
            StatusCode::NOT_FOUND,
            StatusCode::NOT_FOUND,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::TOO_MANY_REQUESTS,
        ]
    );
}
