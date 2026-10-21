//! Accounts of a tenant: directory, creation by an administrator, profile and
//! status changes, primary unit, passwords and session revocation. Accounts
//! are never deleted by the application (audit references, ADR-018).

use chrono::{DateTime, Utc};
use mimorya_core::authz::Sensitivity;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Every status of an account (`users_status_check`).
pub const USER_STATUSES: [&str; 3] = ["active", "invited", "disabled"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DirectoryEntry {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub job_title: Option<String>,
    pub status: String,
    pub clearance: String,
    pub primary_unit_id: Option<Uuid>,
    pub last_login_at: Option<DateTime<Utc>>,
    /// Read by access policies as `subject.attributes.*`.
    pub attributes: Value,
}

const DIRECTORY_SELECT: &str = "select u.id, u.email::text as email, u.display_name, u.job_title, \
    u.status, u.clearance, u.last_login_at, u.attributes, \
    (select m.org_unit_id from org_memberships m where m.user_id = u.id and m.is_primary limit 1) \
      as primary_unit_id \
    from users u";

pub async fn list_users(conn: &mut PgConnection) -> sqlx::Result<Vec<DirectoryEntry>> {
    sqlx::query_as(&format!("{DIRECTORY_SELECT} order by u.display_name"))
        .fetch_all(conn)
        .await
}

pub async fn get_user(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<DirectoryEntry>> {
    sqlx::query_as(&format!("{DIRECTORY_SELECT} where u.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub tenant_id: Uuid,
    /// Lower-cased by the caller; unique across tenants (ADR-011).
    pub email: String,
    pub display_name: String,
    pub job_title: Option<String>,
    pub clearance: Sensitivity,
    /// A temporary password the account must replace, or `None`: the
    /// account is `invited` until it accepts its invitation (ADR-027, ADR-064).
    pub password_hash: Option<String>,
}

pub async fn create_user(conn: &mut PgConnection, input: NewUser) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into users (tenant_id, email, display_name, job_title, clearance, password_hash, \
         must_change_password, status, activated) values ($1, $2, $3, $4, $5, $6, \
         $6 is not null, case when $6 is null then 'invited' else 'active' end, $6 is not null) \
         returning id",
    )
    .bind(input.tenant_id)
    .bind(input.email)
    .bind(input.display_name)
    .bind(input.job_title)
    .bind(input.clearance.as_str())
    .bind(input.password_hash)
    .fetch_one(conn)
    .await
}

/// Fields left to `None` are unchanged; `job_title: Some(None)` clears it.
#[derive(Debug, Clone, Default)]
pub struct UserPatch {
    pub display_name: Option<String>,
    pub job_title: Option<Option<String>>,
    pub clearance: Option<Sensitivity>,
    /// `active` or `disabled`; an account that was never active (its
    /// invitation not accepted yet) goes back to `invited`, not `active`.
    pub status: Option<&'static str>,
    /// Replaces every attribute (checked by the caller).
    pub attributes: Option<Value>,
}

/// Returns whether the account exists (in the transaction's tenant).
pub async fn update_user(
    conn: &mut PgConnection,
    id: Uuid,
    patch: UserPatch,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update users set display_name = coalesce($2, display_name), \
         job_title = case when $3 then $4 else job_title end, \
         clearance = coalesce($5, clearance), \
         status = case when $6 = 'active' and not activated then 'invited' \
           else coalesce($6, status) end, \
         attributes = coalesce($7, attributes), updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(patch.display_name)
    .bind(patch.job_title.is_some())
    .bind(patch.job_title.flatten())
    .bind(patch.clearance.map(Sensitivity::as_str))
    .bind(patch.status)
    .bind(patch.attributes)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Makes `unit` the user's only primary unit (`None`: no primary unit).
pub async fn set_primary_unit(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    unit: Option<Uuid>,
) -> sqlx::Result<()> {
    sqlx::query("delete from org_memberships where user_id = $1 and is_primary")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    if let Some(unit) = unit {
        sqlx::query(
            "insert into org_memberships (tenant_id, user_id, org_unit_id, is_primary) \
             values ($1, $2, $3, true) \
             on conflict (user_id, org_unit_id) do update set is_primary = true",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(unit)
        .execute(conn)
        .await?;
    }
    Ok(())
}

pub async fn get_password_hash(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<String>> {
    let hash: Option<Option<String>> =
        sqlx::query_scalar("select password_hash from users where id = $1")
            .bind(id)
            .fetch_optional(conn)
            .await?;
    Ok(hash.flatten())
}

/// Gives the account a temporary password, which it must replace.
pub async fn set_temporary_password_hash(
    conn: &mut PgConnection,
    id: Uuid,
    hash: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "update users set password_hash = $2, must_change_password = true, updated_at = now() \
         where id = $1",
    )
    .bind(id)
    .bind(hash)
    .execute(conn)
    .await?;
    Ok(())
}

/// The password the person chose through a reset link (ADR-028): no change
/// is required any more.
pub async fn set_reset_password_hash(
    conn: &mut PgConnection,
    id: Uuid,
    hash: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "update users set password_hash = $2, must_change_password = false, updated_at = now() \
         where id = $1",
    )
    .bind(id)
    .bind(hash)
    .execute(conn)
    .await?;
    Ok(())
}

/// Replaces the password only if it is still `current_hash`: a change made in
/// between (an administrator's reset) wins. The new password is the user's
/// own: no change is required any more. Returns whether it was replaced.
pub async fn replace_password_hash(
    conn: &mut PgConnection,
    id: Uuid,
    current_hash: &str,
    new_hash: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update users set password_hash = $3, must_change_password = false, \
         updated_at = now() where id = $1 and password_hash = $2",
    )
    .bind(id)
    .bind(current_hash)
    .bind(new_hash)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Ends every open session of the user, but `except` (the caller's own).
pub async fn revoke_user_sessions(
    conn: &mut PgConnection,
    user_id: Uuid,
    except: Option<Uuid>,
) -> sqlx::Result<u64> {
    let result = sqlx::query(
        "update sessions set revoked_at = now() \
         where user_id = $1 and revoked_at is null and ($2::uuid is null or id <> $2)",
    )
    .bind(user_id)
    .bind(except)
    .execute(conn)
    .await?;
    Ok(result.rows_affected())
}

/// The attribute names in use in the company, sorted: what policy conditions
/// can read under `subject.attributes.`.
pub async fn list_attribute_keys(conn: &mut PgConnection) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "select distinct jsonb_object_keys(attributes) as key from users \
         where jsonb_typeof(attributes) = 'object' order by key",
    )
    .fetch_all(conn)
    .await
}

/// An invited account chooses its password and becomes active; `false` if
/// it is no longer `invited` (disabled meanwhile, or already active).
pub async fn activate_invited_user(
    conn: &mut PgConnection,
    id: Uuid,
    password_hash: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update users set password_hash = $2, must_change_password = false, status = 'active', \
         activated = true, updated_at = now() where id = $1 and status = 'invited'",
    )
    .bind(id)
    .bind(password_hash)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// An invited account accepted through its company's identity provider
/// (ADR-064): active, without a password; whether it was still invited.
pub async fn activate_invited_without_password(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update users set must_change_password = false, status = 'active', activated = true, \
         updated_at = now() where id = $1 and status = 'invited'",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}
