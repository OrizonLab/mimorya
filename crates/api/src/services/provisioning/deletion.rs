//! A suspended company deleted whole by the operator (ADR-192), as the
//! database's owner: the application's role may not delete a company, and
//! no process of the application does. Named by its slug, the company is
//! locked for the transaction, so that a reactivation waits for it or wins
//! before it; it must be suspended (ADR-191), with no run of it queued or
//! under way. Asked without its slug confirmed, the command shows what the
//! company holds, table by table, and deletes nothing; confirmed, it
//! deletes the company and every row of it in one transaction, its audit
//! log included, and tells every process to let go of what it keeps of it.
//! What it held stays in the backups made before, until they are retired.

use mimorya_core::DomainError;
use mimorya_db::repositories::tenant_deletion::{
    delete_tenant, living_runs, lock_tenant_by_slug, tenant_rows,
};
use sqlx::PgPool;
use uuid::Uuid;

use super::suspension::release_everywhere;

/// What a company holds: the rows of each table that names it.
#[derive(Debug)]
pub struct CompanyRows {
    pub tenant_id: Uuid,
    /// The tables holding rows of it, by name, and how many.
    pub tables: Vec<(String, i64)>,
}

impl CompanyRows {
    pub fn total(&self) -> i64 {
        self.tables.iter().map(|(_, rows)| rows).sum()
    }
}

/// What the command did.
#[derive(Debug)]
pub enum CompanyDeletion {
    /// Asked without the slug confirmed: what would be deleted, nothing
    /// deleted.
    Shown(CompanyRows),
    /// Deleted, with what it held.
    Deleted(CompanyRows),
}

/// Shows, or deletes once `confirm` repeats its slug, the suspended company
/// whose slug is `slug`, as the database's `owner`.
pub async fn delete_company(
    owner: &PgPool,
    slug: &str,
    confirm: Option<&str>,
) -> anyhow::Result<CompanyDeletion> {
    let slug = slug.trim();
    let mut tx = owner.begin().await?;
    let Some((tenant_id, status)) = lock_tenant_by_slug(&mut tx, slug).await? else {
        return Err(DomainError::not_found("Aucune entreprise ne porte ce sigle").into());
    };
    if status != "suspended" {
        return Err(DomainError::conflict(
            "Seule une entreprise suspendue peut être supprimée : mimorya-admin \
             suspend-company --slug <sigle> d’abord",
        )
        .into());
    }
    if living_runs(&mut tx, tenant_id).await? > 0 {
        return Err(DomainError::conflict(
            "Un run de l’entreprise est encore en file ou en cours : il s’arrête à sa \
             prochaine étape, réessayez dans un instant",
        )
        .into());
    }
    let rows = CompanyRows {
        tenant_id,
        tables: tenant_rows(&mut tx, tenant_id).await?,
    };
    match confirm.map(str::trim) {
        None => Ok(CompanyDeletion::Shown(rows)),
        Some(confirmed) if confirmed != slug => Err(DomainError::validation(
            "Le sigle confirmé n’est pas celui de l’entreprise : rien n’a été supprimé",
        )
        .into()),
        Some(_) => {
            // Read before the company goes, announced once it is gone.
            release_everywhere(&mut tx, tenant_id).await?;
            if !delete_tenant(&mut tx, tenant_id).await? {
                return Err(DomainError::not_found("Aucune entreprise ne porte ce sigle").into());
            }
            tx.commit().await?;
            Ok(CompanyDeletion::Deleted(rows))
        }
    }
}
