//! A company suspended or reactivated by the operator (ADR-191), named by
//! its slug: its status changed in its own transaction with its audit
//! entry, by the `system` actor, and every process told, once the change
//! committed, to let go of what it keeps of the company (its connectors'
//! instances and the sessions they hold, its access, its model and
//! embedding settings with their keys). Everything else follows from the
//! status, which each path already reads: suspended, a company signs nobody
//! in, its sessions and tokens are refused, its runs stop at their next
//! step, nothing is launched for it and no notice is sent. Its messaging
//! gateways still answer: a message says the company is suspended, and a
//! run stopped by the suspension has its end written in its thread. The
//! command deletes nothing, but freezes nothing either: a request for
//! approval past its deadline still expires (its run, taken up, stops at
//! once) and retention still applies. Reactivated, the company finds its
//! accounts, its data and its sessions not yet expired, less what those
//! deadlines took meanwhile.

use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::notifications::notify_invalidation;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::tenant_connector_ids;
use mimorya_db::repositories::tenants::{change_tenant_status, find_tenant_by_slug, tenant_status};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::system_entry;
use crate::services::invalidations::{Invalidation, announcement};

/// What the operator asks of a company.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanyStatusChange {
    Suspend,
    Reactivate,
}

impl CompanyStatusChange {
    /// The status the company leaves, and the one it takes.
    fn statuses(self) -> (&'static str, &'static str) {
        match self {
            Self::Suspend => ("active", "suspended"),
            Self::Reactivate => ("suspended", "active"),
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::Suspend => "tenant.suspended",
            Self::Reactivate => "tenant.reactivated",
        }
    }

    /// The refusal of a company already in the status asked.
    fn already(self) -> &'static str {
        match self {
            Self::Suspend => "Cette entreprise est déjà suspendue",
            Self::Reactivate => "Cette entreprise n’est pas suspendue",
        }
    }
}

/// Suspends or reactivates the company whose slug is `slug`: its identifier.
pub async fn change_company_status(
    db: &Database,
    slug: &str,
    change: CompanyStatusChange,
) -> anyhow::Result<Uuid> {
    let slug = slug.trim();
    let unknown = || DomainError::not_found("Aucune entreprise ne porte ce sigle");
    let tenant_id = find_tenant_by_slug(db.pool(), slug)
        .await?
        .ok_or_else(unknown)?;
    let (from, to) = change.statuses();
    let mut tx = db.begin(tenant_id).await?;
    match tenant_status(&mut tx, tenant_id).await? {
        None => return Err(unknown().into()),
        Some(status) if status != from => {
            return Err(DomainError::conflict(change.already()).into());
        }
        Some(_) => {}
    }
    // Changed meanwhile by another command, or deleted by the operator
    // (ADR-192): refused as well, for what it now is.
    if !change_tenant_status(&mut tx, tenant_id, from, to).await? {
        return Err(match tenant_status(&mut tx, tenant_id).await? {
            None => unknown(),
            Some(_) => DomainError::conflict(change.already()),
        }
        .into());
    }
    append_audit(
        &mut tx,
        system_entry(tenant_id, change.action(), "tenant", tenant_id)
            .metadata(json!({ "slug": slug })),
    )
    .await?;
    release_everywhere(&mut tx, tenant_id).await?;
    tx.commit().await?;
    Ok(tenant_id)
}

/// Tells every process, once the transaction commits, to let go of what it
/// keeps of the company: announced from the command's own origin, which no
/// process shares, each of them applies it. The company named explicitly:
/// as the database's owner too, whom row-level security does not bound.
pub(super) async fn release_everywhere(
    conn: &mut PgConnection,
    tenant_id: Uuid,
) -> sqlx::Result<()> {
    let origin = Uuid::new_v4();
    let mut released = vec![
        Invalidation::Access(tenant_id),
        Invalidation::Llm(tenant_id),
        Invalidation::Embeddings(tenant_id),
    ];
    for connector in tenant_connector_ids(conn, tenant_id).await? {
        released.push(Invalidation::Connector(connector));
    }
    for invalidation in released {
        notify_invalidation(&mut *conn, &announcement(origin, invalidation)).await?;
    }
    Ok(())
}
