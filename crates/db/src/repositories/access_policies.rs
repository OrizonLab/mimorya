//! Access policies of a tenant as administered: all of them, enabled or not.
//! Administrators write deny policies directly (ADR-022): those writes never
//! touch an allow policy. An allow policy is written only once a proposal is
//! approved (ADR-168, `policy_proposals.rs`), and otherwise only switched off
//! or deleted. Reading the enabled ones for authorization lives in `org.rs`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// What a policy does (`access_policies_effect_check`).
pub const POLICY_EFFECTS: [&str; 2] = ["allow", "deny"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PolicyRow {
    pub id: Uuid,
    pub description: String,
    /// `allow` or `deny`.
    pub effect: String,
    pub permissions: Vec<String>,
    pub condition: Value,
    pub enabled: bool,
    pub updated_at: DateTime<Utc>,
}

const POLICY_SELECT: &str = "select id, description, effect, permissions, condition, enabled, \
    updated_at from access_policies";

pub async fn list_policies(conn: &mut PgConnection) -> sqlx::Result<Vec<PolicyRow>> {
    sqlx::query_as(&format!("{POLICY_SELECT} order by created_at"))
        .fetch_all(conn)
        .await
}

pub async fn get_policy(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<PolicyRow>> {
    sqlx::query_as(&format!("{POLICY_SELECT} where id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// A policy as written: its effect is the function's that writes it.
#[derive(Debug, Clone)]
pub struct NewPolicy {
    pub tenant_id: Uuid,
    pub description: String,
    pub permissions: Vec<String>,
    /// A serialized `mimorya_core::authz::Condition`, checked by the caller.
    pub condition: Value,
    pub enabled: bool,
}

pub async fn create_deny_policy(conn: &mut PgConnection, input: NewPolicy) -> sqlx::Result<Uuid> {
    create_policy(conn, "deny", input).await
}

/// Only from an approved proposal (ADR-168).
pub async fn create_allow_policy(conn: &mut PgConnection, input: NewPolicy) -> sqlx::Result<Uuid> {
    create_policy(conn, "allow", input).await
}

async fn create_policy(
    conn: &mut PgConnection,
    effect: &str,
    input: NewPolicy,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into access_policies (tenant_id, description, effect, permissions, condition, \
         enabled) values ($1, $2, $6, $3, $4, $5) returning id",
    )
    .bind(input.tenant_id)
    .bind(input.description)
    .bind(input.permissions)
    .bind(input.condition)
    .bind(input.enabled)
    .bind(effect)
    .fetch_one(conn)
    .await
}

/// Fields left to `None` are unchanged.
#[derive(Debug, Clone, Default)]
pub struct PolicyPatch {
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
    pub condition: Option<Value>,
    pub enabled: Option<bool>,
}

/// Returns whether a deny policy was changed.
pub async fn update_deny_policy(
    conn: &mut PgConnection,
    id: Uuid,
    patch: PolicyPatch,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update access_policies set description = coalesce($2, description), \
         permissions = coalesce($3, permissions), condition = coalesce($4, condition), \
         enabled = coalesce($5, enabled), updated_at = now() \
         where id = $1 and effect = 'deny'",
    )
    .bind(id)
    .bind(patch.description)
    .bind(patch.permissions)
    .bind(patch.condition)
    .bind(patch.enabled)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Returns whether a deny policy was deleted.
pub async fn delete_deny_policy(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from access_policies where id = $1 and effect = 'deny'")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Writes the allow policy `id` as `input` says, only from an approved
/// proposal (ADR-168) and while the policy was last changed at
/// `updated_at`: returns whether it was.
pub async fn replace_allow_policy(
    conn: &mut PgConnection,
    id: Uuid,
    updated_at: DateTime<Utc>,
    input: NewPolicy,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update access_policies set description = $3, permissions = $4, condition = $5, \
         enabled = $6, updated_at = now() \
         where id = $1 and effect = 'allow' and updated_at = $2",
    )
    .bind(id)
    .bind(updated_at)
    .bind(input.description)
    .bind(input.permissions)
    .bind(input.condition)
    .bind(input.enabled)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Switches the allow policy `id` off, which only takes access away: returns
/// whether it is an allow policy.
pub async fn switch_off_allow_policy(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update access_policies set enabled = false, updated_at = now() \
         where id = $1 and effect = 'allow'",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Returns whether an allow policy was deleted, its proposals with it.
pub async fn delete_allow_policy(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from access_policies where id = $1 and effect = 'allow'")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}
