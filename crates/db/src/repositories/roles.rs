//! Roles of a tenant as administered: the platform roles (fixed) and the
//! custom roles an administrator creates, edits and deletes, with how many
//! assignments use each. Reading them for authorization lives in `org.rs`.

use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoleRow {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub is_system: bool,
    /// Assignments of the role, expired ones included.
    pub assignments: i64,
    /// Those on a unit or on one's own data rather than the whole company.
    pub scoped_assignments: i64,
}

const ROLE_SELECT: &str = "select r.id, r.key, r.name, r.description, r.permissions, r.is_system, \
    (select count(*) from role_assignments a where a.role_id = r.id) as assignments, \
    (select count(*) from role_assignments a where a.role_id = r.id and a.scope_type <> 'tenant') \
      as scoped_assignments \
    from roles r";

pub async fn list_roles(conn: &mut PgConnection) -> sqlx::Result<Vec<RoleRow>> {
    sqlx::query_as(&format!("{ROLE_SELECT} order by r.is_system desc, r.name"))
        .fetch_all(conn)
        .await
}

pub async fn get_role(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<RoleRow>> {
    sqlx::query_as(&format!("{ROLE_SELECT} where r.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// Whether another role of the tenant already has this name (case ignored).
pub async fn role_name_taken(
    conn: &mut PgConnection,
    name: &str,
    except: Option<Uuid>,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "select exists (select 1 from roles where lower(name) = lower($1) \
         and ($2::uuid is null or id <> $2))",
    )
    .bind(name)
    .bind(except)
    .fetch_one(conn)
    .await
}

pub async fn role_key_taken(conn: &mut PgConnection, key: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("select exists (select 1 from roles where key = $1)")
        .bind(key)
        .fetch_one(conn)
        .await
}

#[derive(Debug, Clone)]
pub struct NewRole {
    pub tenant_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
}

pub async fn create_role(conn: &mut PgConnection, input: NewRole) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into roles (tenant_id, key, name, description, permissions, is_system) \
         values ($1, $2, $3, $4, $5, false) returning id",
    )
    .bind(input.tenant_id)
    .bind(input.key)
    .bind(input.name)
    .bind(input.description)
    .bind(input.permissions)
    .fetch_one(conn)
    .await
}

/// Fields left to `None` are unchanged.
#[derive(Debug, Clone, Default)]
pub struct RolePatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
}

/// Platform roles are never changed. Returns whether a custom role was.
pub async fn update_custom_role(
    conn: &mut PgConnection,
    id: Uuid,
    patch: RolePatch,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update roles set name = coalesce($2, name), description = coalesce($3, description), \
         permissions = coalesce($4, permissions), updated_at = now() \
         where id = $1 and not is_system",
    )
    .bind(id)
    .bind(patch.name)
    .bind(patch.description)
    .bind(patch.permissions)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Platform roles are never deleted. Returns whether a custom role was.
pub async fn delete_custom_role(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from roles where id = $1 and not is_system")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}
