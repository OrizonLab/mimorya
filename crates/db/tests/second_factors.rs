//! Second factors and sign-ins waiting for one in the database (ADR-200):
//! a company cannot store, read, use or delete another company's, nor name
//! another company's account in its own; a password or status change ends
//! the changed account's challenges alone; the maintenance purges what is
//! left waiting, in every company.

mod support;

use chrono::{Duration, Utc};
use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::repositories::login_challenges::{
    ChallengePurpose, NewLoginChallenge, find_login_challenge, purge_second_factor_leftovers,
    put_login_challenge,
};
use mimorya_db::repositories::second_factors::{
    accept_step, delete_second_factor, enable_second_factor, lock_second_factor,
    put_pending_second_factor, replace_recovery_codes, use_recovery_code,
};
use mimorya_db::repositories::secrets::StoredSecret;
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

fn sealed() -> StoredSecret {
    StoredSecret {
        ciphertext: vec![1; 32],
        iv: vec![2; 12],
        auth_tag: vec![3; 16],
        key_version: 1,
    }
}

/// An enabled second factor of `user_id`, with one recovery code.
async fn enrolled(db: &Database, tenant_id: Uuid, user_id: Uuid, code: &[u8]) {
    let mut tx = db.begin(tenant_id).await.unwrap();
    assert!(
        put_pending_second_factor(&mut tx, tenant_id, user_id, &sealed())
            .await
            .unwrap()
    );
    assert!(enable_second_factor(&mut tx, user_id, 10).await.unwrap());
    replace_recovery_codes(&mut tx, tenant_id, user_id, &[code.to_vec()])
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn challenge(db: &Database, tenant_id: Uuid, user_id: Uuid, token_hash: &str) {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let challenge = NewLoginChallenge {
        tenant_id,
        user_id,
        token_hash,
        purpose: ChallengePurpose::Verify,
        expires_at: Utc::now() + Duration::minutes(5),
    };
    put_login_challenge(&mut tx, challenge).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_second_factor_stays_within_its_company() {
    let db = app().await;
    let code = [7u8; 32];
    enrolled(&db, OTHER_TENANT_ID, OTHER_USER_ID, &code).await;

    let mut tx = db.begin(tenant::ID).await.unwrap();
    for table in ["second_factors", "second_factor_recovery_codes"] {
        let seen: i64 = sqlx::query_scalar(&format!("select count(*) from {table}"))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(seen, 0, "{table}");
    }
    // Neither read, nor used, nor deleted from another company.
    assert!(
        lock_second_factor(&mut tx, OTHER_USER_ID)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !use_recovery_code(&mut tx, OTHER_USER_ID, &code)
            .await
            .unwrap()
    );
    assert!(!accept_step(&mut tx, OTHER_USER_ID, 20).await.unwrap());
    assert!(!delete_second_factor(&mut tx, OTHER_USER_ID).await.unwrap());
    // Nor written for its account.
    let foreign =
        put_pending_second_factor(&mut tx, OTHER_TENANT_ID, OTHER_USER_ID, &sealed()).await;
    assert!(support::db_error(foreign).contains("row-level security"));
    drop(tx);

    // Nor its own row naming the other company's account.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let crossed = put_pending_second_factor(&mut tx, tenant::ID, OTHER_USER_ID, &sealed()).await;
    assert!(support::db_error(crossed).contains("foreign key"));
    drop(tx);

    // The other company's factor is whole.
    let mut tx = db.begin(OTHER_TENANT_ID).await.unwrap();
    let kept = lock_second_factor(&mut tx, OTHER_USER_ID)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((kept.enabled_at.is_some(), kept.last_step), (true, 10));
    assert!(
        use_recovery_code(&mut tx, OTHER_USER_ID, &code)
            .await
            .unwrap()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_challenge_ends_with_its_account_s_password_alone() {
    let db = app().await;
    challenge(&db, OTHER_TENANT_ID, OTHER_USER_ID, "defi-autre").await;
    challenge(&db, tenant::ID, user::EMMA, "defi-emma").await;
    challenge(&db, tenant::ID, user::HUGO, "defi-hugo").await;

    // Read before sign-in with what decides whether it holds.
    let found = find_login_challenge(db.pool(), "defi-emma")
        .await
        .unwrap()
        .unwrap();
    assert_eq!((found.tenant_id, found.user_id), (tenant::ID, user::EMMA));
    assert_eq!(
        (
            found.purpose.as_str(),
            found.user_status.as_str(),
            found.tenant_status.as_str()
        ),
        ("verify", "active", "active")
    );
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let seen: i64 = sqlx::query_scalar("select count(*) from login_challenges")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(seen, 2);
    sqlx::query("update users set password_hash = 'changé' where id = $1")
        .bind(user::EMMA)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        find_login_challenge(db.pool(), "defi-emma")
            .await
            .unwrap()
            .is_none()
    );
    for kept in ["defi-hugo", "defi-autre"] {
        assert!(
            find_login_challenge(db.pool(), kept)
                .await
                .unwrap()
                .is_some(),
            "{kept}"
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_maintenance_purges_what_is_left_waiting_in_every_company() {
    let db = app().await;
    challenge(&db, OTHER_TENANT_ID, OTHER_USER_ID, "defi-expire").await;
    challenge(&db, tenant::ID, user::JULIEN, "defi-en-cours").await;
    let mut pending = db.begin(tenant::ID).await.unwrap();
    for person in [user::LEA, user::MARC] {
        put_pending_second_factor(&mut pending, tenant::ID, person, &sealed())
            .await
            .unwrap();
    }
    pending.commit().await.unwrap();
    enrolled(&db, tenant::ID, user::CLAIRE, &[9u8; 32]).await;
    // Aged as an operator would find them.
    let owner = sqlx::PgPool::connect(&support::urls(|_| {}).owner)
        .await
        .unwrap();
    sqlx::query("update login_challenges set expires_at = now() - interval '1 second' where token_hash = 'defi-expire'")
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query(
        "update second_factors set created_at = now() - interval '2 days' \
         where user_id = any($1)",
    )
    .bind([user::LEA, user::CLAIRE])
    .execute(&owner)
    .await
    .unwrap();

    assert_eq!(purge_second_factor_leftovers(db.pool()).await.unwrap(), 2);
    assert!(
        find_login_challenge(db.pool(), "defi-expire")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        find_login_challenge(db.pool(), "defi-en-cours")
            .await
            .unwrap()
            .is_some()
    );
    // The enrolment waiting a day goes; the newer one and the one turned on
    // stay, however old.
    let left: Vec<Uuid> = sqlx::query_scalar("select user_id from second_factors order by user_id")
        .fetch_all(&owner)
        .await
        .unwrap();
    assert!(!left.contains(&user::LEA));
    assert!(left.contains(&user::MARC) && left.contains(&user::CLAIRE));
    owner.close().await;
}
