//! The audit log's seal as the database guards it (ADR-178): the
//! application writes entries but never a seal; a seal is set only by
//! `system_seal_audit_entries`, in its company's transaction, once, in
//! order after the company's last place, on an unsealed entry of that
//! company; anything else raises and sets none.

mod support;

use mimorya_core::demo::tenant;
use mimorya_db::repositories::audit_chain::{
    Seal, audit_chain_head, audit_log_tenants, seal_audit_entries, unsealed_audit_entries,
};
use mimorya_db::testing::OTHER_TENANT_ID;
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

async fn append(db: &Database, tenant_id: Uuid, action: &str) -> i64 {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let id = sqlx::query_scalar(
        "insert into audit_log (tenant_id, actor_type, action, outcome) \
         values ($1, 'system', $2, 'success') returning id",
    )
    .bind(tenant_id)
    .bind(action)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// The company's last place, 0 before the first.
async fn head(tx: &mut TenantTx) -> i64 {
    audit_chain_head(tx)
        .await
        .unwrap()
        .map_or(0, |head| head.chain_seq)
}

fn seal(id: i64, seq: i64) -> Seal {
    Seal {
        id,
        seq,
        hash: vec![7; 32],
    }
}

/// The message of a seal refused, its transaction then dropped.
async fn refused(db: &Database, tenant_id: Uuid, seals: &[Seal]) -> String {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    support::db_error(seal_audit_entries(&mut tx, tenant_id, seals).await)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_application_writes_entries_but_never_a_seal() {
    let db = app().await;
    let id = append(&db, tenant::ID, "test.chain.written").await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let sealed_insert = sqlx::query(
        "insert into audit_log (tenant_id, actor_type, action, outcome, chain_seq, chain_hash) \
         values ($1, 'system', 'test.chain.forged', 'success', 1000000, $2)",
    )
    .bind(tenant::ID)
    .bind(vec![7_u8; 32])
    .execute(&mut *tx)
    .await;
    assert!(support::db_error(sealed_insert).contains("permission denied"));
    drop(tx);
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let sealed_update = sqlx::query("update audit_log set chain_seq = 1000000 where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await;
    assert!(support::db_error(sealed_update).contains("permission denied"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_seal_follows_its_company_s_last_place_or_none_is_set() {
    let db = app().await;
    let first = append(&db, tenant::ID, "test.chain.first").await;
    let second = append(&db, tenant::ID, "test.chain.second").await;
    let elsewhere = append(&db, OTHER_TENANT_ID, "test.chain.elsewhere").await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let last = head(&mut tx).await;
    drop(tx);

    // A place skipped, or one that does not follow the one before.
    let skipped = refused(&db, tenant::ID, &[seal(first, last + 2)]).await;
    assert!(skipped.contains("does not follow"), "{skipped}");
    let gap = refused(
        &db,
        tenant::ID,
        &[seal(first, last + 1), seal(second, last + 3)],
    )
    .await;
    assert!(gap.contains("does not follow"), "{gap}");
    // Another company's entry, named as this one's or sealed in this one's
    // transaction.
    let other = refused(&db, tenant::ID, &[seal(elsewhere, last + 1)]).await;
    assert!(other.contains("not an unsealed entry"), "{other}");
    let foreign = refused(&db, OTHER_TENANT_ID, &[seal(elsewhere, 1)]).await;
    assert!(foreign.contains("own company"), "{foreign}");
    // A seal that is not one.
    let short = Seal {
        hash: vec![7; 31],
        ..seal(first, last + 1)
    };
    let check = refused(&db, tenant::ID, &[short]).await;
    assert!(check.contains("audit_log_chain_check"), "{check}");
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let uneven = sqlx::query("select system_seal_audit_entries($1, $2, $3, $4)")
        .bind(tenant::ID)
        .bind(vec![first])
        .bind(vec![last + 1])
        .bind(Vec::<Vec<u8>>::new())
        .execute(&mut *tx)
        .await;
    assert!(support::db_error(uneven).contains("as many places"));
    drop(tx);

    // None of these sealed anything.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    assert_eq!(head(&mut tx).await, last);
    let unsealed: Vec<i64> = unsealed_audit_entries(&mut tx, 1000)
        .await
        .unwrap()
        .iter()
        .map(|entry| entry.id)
        .collect();
    assert!(unsealed.contains(&first) && unsealed.contains(&second));
    let sealed = seal_audit_entries(
        &mut tx,
        tenant::ID,
        &[seal(first, last + 1), seal(second, last + 2)],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(sealed, 2);

    // Sealed once only.
    let again = refused(&db, tenant::ID, &[seal(first, last + 3)]).await;
    assert!(again.contains("not an unsealed entry"), "{again}");
    // The other company's entry waits for its own chain.
    assert!(
        audit_log_tenants(db.pool(), true)
            .await
            .unwrap()
            .contains(&OTHER_TENANT_ID)
    );
    let mut tx = db.begin(OTHER_TENANT_ID).await.unwrap();
    let theirs = unsealed_audit_entries(&mut tx, 1000).await.unwrap();
    assert!(theirs.iter().any(|entry| entry.id == elsewhere));
}
