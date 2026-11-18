//! How a company's model is supplied (ADR-208): the application's role only
//! reads the formulas, the credits and what was purged of the prepaid
//! consumption, its own company's alone, and may write none of them; the
//! credit's register is in append only, even for the owner, but a company
//! deleted whole takes it along; the table keeps a formula's cap coherent;
//! and the consumption purged after five years leaves a prepaid company's
//! balance where it was, as its runs read it before a call.

mod support;

use chrono::{Duration, Utc};
use mimorya_core::demo::tenant;
use mimorya_core::model_supply::{SupplyFormula, SupplyStop, supply_stop};
use mimorya_db::Database;
use mimorya_db::repositories::model_supply::{
    NewSupplyFormula, add_credit_entry, model_supply, set_supply_formula, supply_spending,
    supply_state,
};
use mimorya_db::testing::OTHER_TENANT_ID;
use sqlx::PgPool;
use support::db_error;
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

async fn owner() -> PgPool {
    PgPool::connect(&support::urls(|_| {}).owner).await.unwrap()
}

/// A company of its own for the test, deleted at its end.
async fn company(owner: &PgPool, slug: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("insert into tenants (id, slug, name) values ($1, $2, $2)")
        .bind(id)
        .bind(slug)
        .execute(owner)
        .await
        .unwrap();
    id
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_application_only_reads_its_own_company_s_supply() {
    let owner = owner().await;
    add_credit_entry(
        &mut owner.acquire().await.unwrap(),
        OTHER_TENANT_ID,
        1_000_000,
        "FAC-AUTRE",
    )
    .await
    .unwrap();
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    // The demo company, supplied by the seed, is read; the other's credit
    // is not seen, even named.
    let supply = model_supply(&mut tx, tenant::ID).await.unwrap().unwrap();
    assert_eq!(supply.formula, SupplyFormula::Capped.as_str());
    let others: i64 =
        sqlx::query_scalar("select count(*) from tenant_credit_entries where tenant_id = $1")
            .bind(OTHER_TENANT_ID)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(others, 0);
    tx.commit().await.unwrap();
    // Nothing of the operator's is the application's to write.
    for statement in [
        "insert into tenant_model_supply (tenant_id, formula) values ($1, 'prepaid')",
        "update tenant_model_supply set unlimited = true, monthly_cap_micros = null \
         where tenant_id = $1",
        "delete from tenant_model_supply where tenant_id = $1",
        "insert into tenant_credit_entries (tenant_id, amount_micros, reference) \
         values ($1, 1000000000, 'FAC-FORGEE')",
        "insert into tenant_prepaid_purged (tenant_id, billed_micros) values ($1, -1000000)",
    ] {
        let mut tx = db.begin(tenant::ID).await.unwrap();
        let refused = db_error(
            sqlx::query(statement)
                .bind(tenant::ID)
                .execute(&mut *tx)
                .await,
        );
        assert!(
            refused.contains("permission denied"),
            "{statement}: {refused}"
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_credit_register_is_append_only_even_for_the_owner() {
    let owner = owner().await;
    let id = company(&owner, "registre-credit").await;
    let mut conn = owner.acquire().await.unwrap();
    assert!(
        add_credit_entry(&mut conn, id, 500_000_000, "FAC-1")
            .await
            .unwrap()
    );
    // The same invoice twice: not recorded again.
    assert!(
        !add_credit_entry(&mut conn, id, 500_000_000, "FAC-1")
            .await
            .unwrap()
    );
    // A correction is an entry of its own.
    assert!(
        add_credit_entry(&mut conn, id, -20_000_000, "AV-1")
            .await
            .unwrap()
    );
    for statement in [
        "update tenant_credit_entries set amount_micros = 900000000 where tenant_id = $1",
        "delete from tenant_credit_entries where tenant_id = $1",
    ] {
        let refused = db_error(sqlx::query(statement).bind(id).execute(&mut *conn).await);
        assert!(
            refused.contains("never changed nor deleted"),
            "{statement}: {refused}"
        );
    }
    let refused = db_error(
        sqlx::query("truncate tenant_credit_entries")
            .execute(&mut *conn)
            .await,
    );
    assert!(refused.contains("never changed nor deleted"), "{refused}");
    let spent = supply_spending(&mut conn, id, Utc::now()).await.unwrap();
    assert_eq!(spent.balance_micros(), 480_000_000);
    // Deleted whole, the company takes its register along (ADR-192).
    sqlx::query("delete from tenants where id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .unwrap();
    let left: i64 =
        sqlx::query_scalar("select count(*) from tenant_credit_entries where tenant_id = $1")
            .bind(id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_table_keeps_each_formula_s_cap_coherent() {
    let owner = owner().await;
    let id = company(&owner, "formule-coherente").await;
    for (values, constraint) in [
        ("'capped', null, false, 0", "tenant_model_supply_capped"),
        ("'capped', 5000000, true, 0", "tenant_model_supply_capped"),
        ("'prepaid', 5000000, false, 0", "tenant_model_supply_capped"),
        ("'client_key', null, true, 0", "tenant_model_supply_capped"),
        ("'capped', 999999, false, 0", "tenant_model_supply_cap"),
        (
            "'capped', 100000000001, false, 0",
            "tenant_model_supply_cap",
        ),
        (
            "'prepaid', null, false, 10001",
            "tenant_model_supply_markup",
        ),
        ("'gratuit', null, false, 0", "tenant_model_supply_formula"),
    ] {
        let refused = db_error(
            sqlx::query(&format!(
                "insert into tenant_model_supply \
                 (tenant_id, formula, monthly_cap_micros, unlimited, markup_bps) \
                 values ($1, {values})"
            ))
            .bind(id)
            .execute(&owner)
            .await,
        );
        assert!(refused.contains(constraint), "{values}: {refused}");
    }
    for values in ["'capped', 1000000, false, 0", "'capped', null, true, 10000"] {
        sqlx::query(&format!(
            "insert into tenant_model_supply \
             (tenant_id, formula, monthly_cap_micros, unlimited, markup_bps) \
             values ($1, {values}) on conflict (tenant_id) do update \
             set formula = excluded.formula, monthly_cap_micros = excluded.monthly_cap_micros, \
             unlimited = excluded.unlimited, markup_bps = excluded.markup_bps"
        ))
        .bind(id)
        .execute(&owner)
        .await
        .unwrap();
    }
    sqlx::query("delete from tenants where id = $1")
        .bind(id)
        .execute(&owner)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_consumption_purged_after_five_years_leaves_the_balance_where_it_was() {
    let owner = owner().await;
    let id = company(&owner, "credit-ancien").await;
    let mut conn = owner.acquire().await.unwrap();
    assert!(
        add_credit_entry(&mut conn, id, 100_000_000, "FAC-2020")
            .await
            .unwrap()
    );
    // Two prepaid calls long past, one recent, and a company-key one long past.
    let old = Utc::now() - Duration::days(6 * 366);
    for (formula, billed, at) in [
        ("prepaid", Some(30_000_000_i64), old),
        ("prepaid", Some(10_000_000), old),
        ("prepaid", Some(5_000_000), Utc::now()),
        ("client_key", None, old),
    ] {
        sqlx::query(
            "insert into usage_records (tenant_id, kind, provider, model, cost_micros, \
             supply_formula, billed_micros, created_at) \
             values ($1, 'llm', 'anthropic', 'claude-haiku-5-5', 1, $2, $3, $4)",
        )
        .bind(id)
        .bind(formula)
        .bind(billed)
        .bind(at)
        .execute(&mut *conn)
        .await
        .unwrap();
    }
    let before = supply_spending(&mut conn, id, Utc::now()).await.unwrap();
    assert_eq!(before.balance_micros(), 55_000_000);
    let purged: Vec<(Uuid, i32)> = sqlx::query_as("select * from system_purge_old_usage(5000)")
        .fetch_all(&mut *conn)
        .await
        .unwrap();
    assert!(purged.contains(&(id, 3)), "{purged:?}");
    let left: i64 = sqlx::query_scalar("select count(*) from usage_records where tenant_id = $1")
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(left, 1);
    let after = supply_spending(&mut conn, id, Utc::now()).await.unwrap();
    assert_eq!(after.balance_micros(), 55_000_000);
    assert_eq!(after.prepaid_billed_ever_micros, 45_000_000);

    // As a run reads it before a call (ADR-208, step 1): the balance the
    // same, spent once the consumption comes to the payments.
    let prepaid = NewSupplyFormula {
        formula: SupplyFormula::Prepaid,
        monthly_cap_micros: None,
        unlimited: false,
    };
    set_supply_formula(&mut conn, id, prepaid).await.unwrap();
    let state = supply_state(&mut conn, id, Utc::now()).await.unwrap();
    assert_eq!(
        (state.formula, state.prepaid_balance_micros),
        (SupplyFormula::Prepaid, 55_000_000)
    );
    assert_eq!(supply_stop(&state), None);
    sqlx::query(
        "insert into usage_records (tenant_id, kind, provider, model, cost_micros, \
         supply_formula, billed_micros) \
         values ($1, 'llm', 'anthropic', 'claude-haiku-5-5', 1, 'prepaid', 55000000)",
    )
    .bind(id)
    .execute(&mut *conn)
    .await
    .unwrap();
    let state = supply_state(&mut conn, id, Utc::now()).await.unwrap();
    assert_eq!(state.prepaid_balance_micros, 0);
    assert_eq!(supply_stop(&state), Some(SupplyStop::CreditSpent));
    sqlx::query("delete from tenants where id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .unwrap();
}
