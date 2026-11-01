//! What the evaluations of a company's skills cost, day by day (ADR-154):
//! each cost added to today's, the days past the window not counted, those
//! long past deleted as new ones are written, another company's apart. The
//! limit a company sets stays within the platform's bounds. Each test
//! leaves its transaction uncommitted: it is rolled back.

mod support;

use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::evaluation_settings::{
    evaluation_limit, remove_evaluation_limit, set_evaluation_limit,
};
use mimorya_db::repositories::evaluation_spending::{add_evaluation_spending, evaluation_spending};
use mimorya_db::testing::OTHER_TENANT_ID;
use mimorya_db::{Database, TenantTx};

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

/// A day of spending `days_ago` days back, written directly.
async fn spent_on(tx: &mut TenantTx, days_ago: i32, cost_micros: i64) {
    sqlx::query(
        "insert into skill_evaluation_spending (tenant_id, day, cost_micros) \
         values ($1, (now() at time zone 'utc')::date - $2, $3)",
    )
    .bind(tenant::ID)
    .bind(days_ago)
    .bind(cost_micros)
    .execute(&mut **tx)
    .await
    .unwrap();
}

async fn days(tx: &mut TenantTx) -> Vec<i32> {
    sqlx::query_scalar(
        "select ((now() at time zone 'utc')::date - day)::int from skill_evaluation_spending \
         order by day desc",
    )
    .fetch_all(&mut **tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_cost_adds_to_today_s_and_the_window_counts_thirty_days() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    spent_on(&mut tx, 29, 1_000_000).await;
    spent_on(&mut tx, 30, 2_000_000).await;
    spent_on(&mut tx, 45, 4_000_000).await;
    assert_eq!(evaluation_spending(&mut tx, 30).await.unwrap(), 1_000_000);

    add_evaluation_spending(&mut tx, tenant::ID, 250_000, 40)
        .await
        .unwrap();
    add_evaluation_spending(&mut tx, tenant::ID, 125_000, 40)
        .await
        .unwrap();
    // Nothing negative is counted.
    add_evaluation_spending(&mut tx, tenant::ID, -5_000_000, 40)
        .await
        .unwrap();
    assert_eq!(evaluation_spending(&mut tx, 30).await.unwrap(), 1_375_000);
    assert_eq!(evaluation_spending(&mut tx, 31).await.unwrap(), 3_375_000);
    // The day of 45 days ago went as today's was written.
    assert_eq!(days(&mut tx).await, [0, 29, 30]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_spending_is_its_own() {
    let db = app().await;
    let mut other = db.begin(OTHER_TENANT_ID).await.unwrap();
    add_evaluation_spending(&mut other, OTHER_TENANT_ID, 9_000_000, 40)
        .await
        .unwrap();
    // Written for the demo company from the other's transaction: refused.
    let crossed = add_evaluation_spending(&mut other, tenant::ID, 1, 40).await;
    assert!(crossed.is_err());
    drop(other);

    let mut tx = db.begin(tenant::ID).await.unwrap();
    assert_eq!(evaluation_spending(&mut tx, 30).await.unwrap(), 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_s_limit_stays_within_the_platform_s_bounds() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    assert_eq!(evaluation_limit(&mut tx).await.unwrap(), None);
    set_evaluation_limit(&mut tx, tenant::ID, 1_000_000, user::PAUL)
        .await
        .unwrap();
    set_evaluation_limit(&mut tx, tenant::ID, 1_000_000_000, user::PAUL)
        .await
        .unwrap();
    assert_eq!(
        evaluation_limit(&mut tx).await.unwrap(),
        Some(1_000_000_000)
    );
    assert!(remove_evaluation_limit(&mut tx).await.unwrap());
    assert!(!remove_evaluation_limit(&mut tx).await.unwrap());
    drop(tx);
    for refused in [999_999, 1_000_000_001] {
        let mut tx = db.begin(tenant::ID).await.unwrap();
        let set = set_evaluation_limit(&mut tx, tenant::ID, refused, user::PAUL).await;
        let error = set.expect_err("beyond the bounds").to_string();
        assert!(error.contains("tenant_evaluation_settings_cost"), "{error}");
    }
}
