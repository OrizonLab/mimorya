//! Reset links in the database (ADR-028): a company cannot store, read or
//! retire another company's links, and the trigger that retires a link on a
//! password or status change stays within the changed account.

mod support;

use chrono::{Duration, Utc};
use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::repositories::password_resets::{
    NewPasswordReset, find_password_reset, put_password_reset,
};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

async fn store(db: &Database, tenant_id: uuid::Uuid, user_id: uuid::Uuid, token_hash: &str) {
    let mut tx = db.begin(tenant_id).await.unwrap();
    put_password_reset(
        &mut tx,
        NewPasswordReset {
            tenant_id,
            user_id,
            token_hash,
            expires_at: Utc::now() + Duration::hours(1),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn links_stay_within_their_company() {
    let db = app().await;
    store(&db, OTHER_TENANT_ID, OTHER_USER_ID, "empreinte-autre").await;

    let mut tx = db.begin(tenant::ID).await.unwrap();
    let seen: i64 = sqlx::query_scalar("select count(*) from password_resets")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(seen, 0);
    // Writing for another company's account is refused.
    let foreign = put_password_reset(
        &mut tx,
        NewPasswordReset {
            tenant_id: OTHER_TENANT_ID,
            user_id: OTHER_USER_ID,
            token_hash: "empreinte-forgee",
            expires_at: Utc::now() + Duration::hours(1),
        },
    )
    .await;
    assert!(support::db_error(foreign).contains("row-level security"));
    drop(tx);

    // A status change in one company leaves the other's links alone.
    store(&db, tenant::ID, user::LUCAS, "empreinte-locale").await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    sqlx::query("update users set status = 'disabled' where id = $1")
        .bind(user::LUCAS)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        find_password_reset(db.pool(), "empreinte-locale")
            .await
            .unwrap()
            .is_none()
    );
    let other = find_password_reset(db.pool(), "empreinte-autre")
        .await
        .unwrap()
        .expect("the other company's link");
    assert_eq!(
        (other.tenant_id, other.user_id),
        (OTHER_TENANT_ID, OTHER_USER_ID)
    );
}
