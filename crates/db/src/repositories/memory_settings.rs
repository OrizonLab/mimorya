//! How a company's memory grows by itself (ADR-076): whether a lesson is
//! drawn from its runs; off without a row.

use sqlx::PgConnection;
use uuid::Uuid;

pub async fn captures_lessons(conn: &mut PgConnection) -> sqlx::Result<bool> {
    let enabled: Option<bool> =
        sqlx::query_scalar("select capture_lessons from tenant_memory_settings")
            .fetch_optional(conn)
            .await?;
    Ok(enabled.unwrap_or(false))
}

pub async fn set_capture_lessons(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    enabled: bool,
    updated_by_user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_memory_settings (tenant_id, capture_lessons, updated_by_user_id) \
         values ($1, $2, $3) \
         on conflict (tenant_id) do update set capture_lessons = excluded.capture_lessons, \
         updated_by_user_id = excluded.updated_by_user_id, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(enabled)
    .bind(updated_by_user_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// The titles of the lessons `user_id` keeps or was proposed in their
/// personal memory, newest first: none is proposed twice.
pub async fn personal_lesson_titles(
    conn: &mut PgConnection,
    user_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "select title from memory_items where scope = 'user' and owner_user_id = $1 \
         and kind = 'lesson' and status in ('active', 'proposed') \
         order by created_at desc limit $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(conn)
    .await
}
