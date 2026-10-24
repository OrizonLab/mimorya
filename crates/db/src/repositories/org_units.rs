//! Changes to the organisation tree. Reading it lives in `org.rs`; the tree
//! rules (no cycle, root fixed) are checked by the caller with
//! `mimorya_core::administration`, on the tree read under the administration
//! lock (`TenantTx::lock_administration`).

use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewOrgUnit {
    pub tenant_id: Uuid,
    pub parent_id: Uuid,
    pub name: String,
    /// One of `OrgUnitType::as_str()`.
    pub unit_type: &'static str,
}

pub async fn create_org_unit(conn: &mut PgConnection, input: NewOrgUnit) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into org_units (tenant_id, parent_id, name, type) values ($1, $2, $3, $4) returning id",
    )
    .bind(input.tenant_id)
    .bind(input.parent_id)
    .bind(input.name)
    .bind(input.unit_type)
    .fetch_one(conn)
    .await
}

/// The root of a new company's tree: the company itself.
pub async fn create_root_unit(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    name: &str,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into org_units (tenant_id, name, type) values ($1, $2, 'company') returning id",
    )
    .bind(tenant_id)
    .bind(name)
    .fetch_one(conn)
    .await
}

/// Fields left to `None` are unchanged; `lead_user_id: Some(None)` clears it.
#[derive(Debug, Clone, Default)]
pub struct OrgUnitPatch {
    pub name: Option<String>,
    pub unit_type: Option<&'static str>,
    pub parent_id: Option<Uuid>,
    pub lead_user_id: Option<Option<Uuid>>,
}

pub async fn update_org_unit(
    conn: &mut PgConnection,
    id: Uuid,
    patch: OrgUnitPatch,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update org_units set name = coalesce($2, name), type = coalesce($3, type), \
         parent_id = coalesce($4, parent_id), \
         lead_user_id = case when $5 then $6 else lead_user_id end, updated_at = now() \
         where id = $1",
    )
    .bind(id)
    .bind(patch.name)
    .bind(patch.unit_type)
    .bind(patch.parent_id)
    .bind(patch.lead_user_id.is_some())
    .bind(patch.lead_user_id.flatten())
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// What still depends on a unit. Deleting it would cascade to memberships,
/// role assignments and team memory: it is refused while any remains.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct OrgUnitUsage {
    pub children: i64,
    pub members: i64,
    pub assignments: i64,
    pub memories: i64,
    pub documents: i64,
}

impl OrgUnitUsage {
    pub fn is_unused(&self) -> bool {
        *self == Self::default()
    }
}

pub async fn org_unit_usage(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<OrgUnitUsage> {
    sqlx::query_as(
        "select (select count(*) from org_units where parent_id = $1) as children, \
         (select count(*) from org_memberships where org_unit_id = $1) as members, \
         (select count(*) from role_assignments where scope_org_unit_id = $1) as assignments, \
         (select count(*) from memory_items where org_unit_id = $1) as memories, \
         (select count(*) from documents where org_unit_id = $1) as documents",
    )
    .bind(id)
    .fetch_one(conn)
    .await
}

/// Locks the unit's row until the end of the transaction: rows that would
/// reference it (memberships, assignments, memory, documents) wait, then
/// see it gone.
/// Returns whether the unit exists.
pub async fn lock_org_unit(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let found: Option<Uuid> =
        sqlx::query_scalar("select id from org_units where id = $1 for update")
            .bind(id)
            .fetch_optional(conn)
            .await?;
    Ok(found.is_some())
}

pub async fn delete_org_unit(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from org_units where id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}
