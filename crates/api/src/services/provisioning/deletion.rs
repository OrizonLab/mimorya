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
//! What the platform declared for it at other services (its MCP servers'
//! OAuth authorizations, its Telegram bots' webhooks) is withdrawn there
//! first, by the release (ADR-193): a confirmed deletion is refused while
//! any is left. What only its people or its own applications can withdraw
//! (linked accounts, Slack and Teams) stays there, said to the operator.

use mimorya_core::DomainError;
use mimorya_core::account_links::LinkKind;
use mimorya_db::repositories::tenant_deletion::{
    delete_tenant, living_work, lock_tenant_by_slug, still_to_release, tenant_rows,
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
    /// What the release withdraws at other services, still held (ADR-193).
    pub to_release: i64,
}

/// What gives access at other services, granted for or by a company, by
/// the table that keeps it. The release withdraws there its OAuth
/// authorizations and its Telegram bots before any deletion (ADR-193); its
/// linked accounts and its Slack and Teams applications stay there, to
/// withdraw there. What it declared there without anyone's grant (clients
/// the platform registered, applications, identity providers) is not
/// counted.
const HELD_OUTSIDE: [(&str, &str); 4] = [
    (
        "connector_authorizations",
        "autorisations OAuth de serveurs MCP données pour l’entreprise",
    ),
    (
        "connector_person_authorizations",
        "autorisations OAuth de serveurs MCP données par ses personnes",
    ),
    (
        "account_links",
        "comptes Microsoft 365 ou Google Workspace reliés par ses personnes",
    ),
    ("gateways", "passerelles de conversation"),
];

impl CompanyRows {
    pub fn total(&self) -> i64 {
        self.tables.iter().map(|(_, rows)| rows).sum()
    }

    /// What it declared at other services, which stays there once it is
    /// deleted: what each is, and how many.
    pub fn held_outside(&self) -> Vec<(&'static str, i64)> {
        HELD_OUTSIDE
            .iter()
            .filter_map(|(table, what)| {
                let rows = self.tables.iter().find(|(name, _)| name == table)?.1;
                Some((*what, rows))
            })
            .collect()
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
    if living_work(&mut tx, tenant_id).await? > 0 {
        return Err(DomainError::conflict(
            "Un run de l’entreprise est encore en file ou en cours, ou une évaluation de ses \
             compétences se joue : ils s’arrêtent bientôt, réessayez dans un instant",
        )
        .into());
    }
    let rows = CompanyRows {
        tenant_id,
        tables: tenant_rows(&mut tx, tenant_id).await?,
        to_release: still_to_release(&mut tx, tenant_id, &LinkKind::revoking_connector_types())
            .await?,
    };
    match confirm.map(str::trim) {
        None => Ok(CompanyDeletion::Shown(rows)),
        Some(confirmed) if confirmed != slug => Err(DomainError::validation(
            "Le sigle confirmé n’est pas celui de l’entreprise : rien n’a été supprimé",
        )
        .into()),
        Some(_) if rows.to_release > 0 => Err(DomainError::conflict(
            "Des autorisations OAuth de ses serveurs MCP, des bots Telegram ou des comptes \
             Google reliés sont encore déclarés chez leurs fournisseurs : mimorya-admin \
             release-company --slug <sigle>, dans le service api, les y retire d’abord",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_what_stays_at_other_services() {
        let rows = CompanyRows {
            tenant_id: Uuid::nil(),
            to_release: 0,
            tables: vec![
                ("account_links".into(), 2),
                ("audit_log".into(), 40),
                ("gateways".into(), 1),
            ],
        };
        assert_eq!(
            rows.held_outside(),
            [
                (
                    "comptes Microsoft 365 ou Google Workspace reliés par ses personnes",
                    2
                ),
                ("passerelles de conversation", 1),
            ]
        );
    }
}
