//! What a company requires of its password accounts (ADR-201): its
//! requirement as stored, none without a row. Read and written in the
//! company's transaction.

use sqlx::PgConnection;
use uuid::Uuid;

/// The company's requirement (`optional`, `administrators`, `everyone`),
/// if it set one.
pub async fn second_factor_requirement(conn: &mut PgConnection) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("select requirement from tenant_second_factor_settings")
        .fetch_optional(conn)
        .await
}

pub async fn set_second_factor_requirement(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    requirement: &str,
    updated_by_user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_second_factor_settings (tenant_id, requirement, updated_by_user_id) \
         values ($1, $2, $3) \
         on conflict (tenant_id) do update set requirement = excluded.requirement, \
         updated_by_user_id = excluded.updated_by_user_id, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(requirement)
    .bind(updated_by_user_id)
    .execute(conn)
    .await?;
    Ok(())
}
