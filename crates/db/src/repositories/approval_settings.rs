//! The risks whose actions a company's managers validate (ADR-059); none
//! without a row.

use sqlx::PgConnection;
use uuid::Uuid;

pub async fn get_manager_risks(conn: &mut PgConnection) -> sqlx::Result<Vec<String>> {
    let risks: Option<Vec<String>> =
        sqlx::query_scalar("select manager_risks from approval_settings")
            .fetch_optional(conn)
            .await?;
    Ok(risks.unwrap_or_default())
}

pub async fn put_manager_risks(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    risks: &[&str],
    updated_by_user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into approval_settings (tenant_id, manager_risks, updated_by_user_id) \
         values ($1, $2, $3) \
         on conflict (tenant_id) do update set manager_risks = excluded.manager_risks, \
         updated_by_user_id = excluded.updated_by_user_id, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(risks)
    .bind(updated_by_user_id)
    .execute(conn)
    .await?;
    Ok(())
}
