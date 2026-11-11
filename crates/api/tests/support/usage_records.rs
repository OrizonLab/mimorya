//! The consumption retention tests' data (ADR-197): records of any age,
//! written as an operator writes them, and what is left of them read back.

use uuid::Uuid;

use super::retention::owner;

/// A consumption record of the company `tenant_id` for `model`, made
/// `months` ago.
pub async fn usage_record(tenant_id: Uuid, model: &str, months: i32) {
    let owner = owner().await;
    sqlx::query(
        "insert into usage_records (tenant_id, kind, provider, model, input_tokens, \
         output_tokens, cost_micros, created_at) \
         values ($1, 'llm', 'anthropic', $2, 10, 5, 100, now() - make_interval(months => $3))",
    )
    .bind(tenant_id)
    .bind(model)
    .bind(months)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

/// `count` consumption records of the company `tenant_id` for `model`, all
/// made `months` ago.
pub async fn usage_records(tenant_id: Uuid, model: &str, count: i32, months: i32) {
    let owner = owner().await;
    sqlx::query(
        "insert into usage_records (tenant_id, kind, provider, model, input_tokens, \
         output_tokens, cost_micros, created_at) \
         select $1, 'llm', 'anthropic', $2, 10, 5, 100, now() - make_interval(months => $4) \
         from generate_series(1, $3)",
    )
    .bind(tenant_id)
    .bind(model)
    .bind(count)
    .bind(months)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

/// The models of the records left that start with `prefix`, sorted.
pub async fn usage_left(prefix: &str) -> Vec<String> {
    let owner = owner().await;
    let mut models: Vec<String> =
        sqlx::query_scalar("select model from usage_records where model like $1 || '%'")
            .bind(prefix)
            .fetch_all(&owner)
            .await
            .unwrap();
    owner.close().await;
    models.sort();
    models
}
