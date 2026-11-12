//! What a company requires of its password accounts (ADR-201), as kept:
//! read back as written, nothing else than its three values. Another
//! company's, recorded, is neither read nor overwritten, and nothing is
//! written for it. Each test leaves its transaction uncommitted, but the one
//! that records another company's row, which deletes it.

mod support;

use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::repositories::second_factor_settings::{
    second_factor_requirement, set_second_factor_requirement,
};
use mimorya_db::testing::OTHER_TENANT_ID;
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_s_requirement_is_read_back_as_written() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    assert_eq!(second_factor_requirement(&mut tx).await.unwrap(), None);
    for requirement in ["administrators", "everyone", "optional"] {
        set_second_factor_requirement(&mut tx, tenant::ID, requirement, user::PAUL)
            .await
            .unwrap();
        assert_eq!(
            second_factor_requirement(&mut tx).await.unwrap().as_deref(),
            Some(requirement)
        );
    }
    drop(tx);
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let refused = set_second_factor_requirement(&mut tx, tenant::ID, "tous", user::PAUL).await;
    let error = refused.expect_err("not a requirement").to_string();
    assert!(
        error.contains("tenant_second_factor_settings_requirement_check"),
        "{error}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_requirement_is_its_own() {
    // The other company requires it of everyone.
    let owner = sqlx::PgPool::connect(&support::urls(|_| {}).owner)
        .await
        .unwrap();
    sqlx::query(
        "insert into tenant_second_factor_settings (tenant_id, requirement) \
         values ($1, 'everyone')",
    )
    .bind(OTHER_TENANT_ID)
    .execute(&owner)
    .await
    .unwrap();

    // The demo company reads none of it, and sets its own beside it.
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    assert_eq!(second_factor_requirement(&mut tx).await.unwrap(), None);
    set_second_factor_requirement(&mut tx, tenant::ID, "administrators", user::PAUL)
        .await
        .unwrap();
    assert_eq!(
        second_factor_requirement(&mut tx).await.unwrap().as_deref(),
        Some("administrators")
    );
    drop(tx);

    // Nor does it write the other's, even naming it.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let crossed =
        set_second_factor_requirement(&mut tx, OTHER_TENANT_ID, "optional", user::PAUL).await;
    let error = crossed.expect_err("another company's").to_string();
    assert!(error.contains("row-level security"), "{error}");
    drop(tx);
    let kept: Vec<(Uuid, String)> =
        sqlx::query_as("select tenant_id, requirement from tenant_second_factor_settings")
            .fetch_all(&owner)
            .await
            .unwrap();
    assert_eq!(kept, [(OTHER_TENANT_ID, "everyone".to_string())]);

    sqlx::query("delete from tenant_second_factor_settings where tenant_id = $1")
        .bind(OTHER_TENANT_ID)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}
