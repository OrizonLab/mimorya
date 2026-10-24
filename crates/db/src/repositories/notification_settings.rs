//! The e-mails each person chooses to receive (ADR-094); all of them
//! without a row.

use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct NotificationSettingsRow {
    /// The requests awaiting their validation, as a manager or standing in.
    pub approvals: bool,
    /// Their scheduled tasks' runs and missions' stages awaiting it.
    pub own_runs: bool,
    /// The end of their missions.
    pub mission_ends: bool,
}

/// Every e-mail: what a person receives until they choose otherwise.
pub const ALL_NOTICES: NotificationSettingsRow = NotificationSettingsRow {
    approvals: true,
    own_runs: true,
    mission_ends: true,
};

/// What `user_id` chose, in the company `conn` reads in.
pub async fn notification_settings(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<NotificationSettingsRow> {
    let chosen = sqlx::query_as(
        "select approvals, own_runs, mission_ends from user_notification_settings \
         where user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(chosen.unwrap_or(ALL_NOTICES))
}

pub async fn set_notification_settings(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    chosen: NotificationSettingsRow,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into user_notification_settings (user_id, tenant_id, approvals, own_runs, \
         mission_ends) values ($1, $2, $3, $4, $5) \
         on conflict (user_id) do update set approvals = excluded.approvals, \
         own_runs = excluded.own_runs, mission_ends = excluded.mission_ends, updated_at = now()",
    )
    .bind(user_id)
    .bind(tenant_id)
    .bind(chosen.approvals)
    .bind(chosen.own_runs)
    .bind(chosen.mission_ends)
    .execute(conn)
    .await?;
    Ok(())
}
