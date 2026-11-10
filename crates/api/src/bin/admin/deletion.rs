//! `mimorya-admin delete-company` (ADR-192): a suspended company deleted
//! whole, as the database's owner (`DATABASE_URL`, as `migrate`), the only
//! role that may. Without its slug confirmed, the operator is shown what
//! the company holds, table by table, and nothing is deleted.

use anyhow::anyhow;
use mimorya_api::services::provisioning::{CompanyDeletion, CompanyRows, delete_company};
use sqlx::postgres::PgPoolOptions;

pub async fn delete(slug: &str, confirm: Option<&str>) -> anyhow::Result<()> {
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow!("DATABASE_URL (rôle propriétaire) est obligatoire"))?;
    let owner = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let done = delete_company(&owner, slug, confirm).await;
    owner.close().await;
    let slug = slug.trim();
    match done? {
        CompanyDeletion::Shown(rows) => {
            println!(
                "Entreprise de sigle {slug} (identifiant {}), suspendue. Elle garde :",
                rows.tenant_id
            );
            println!("{}", held(&rows));
            outside(&rows);
            if rows.to_release > 0 {
                println!(
                    "Avant de la supprimer, retirez chez leurs fournisseurs ses autorisations \
                     OAuth et ses bots Telegram, dans le service api :\n  \
                     mimorya-admin release-company --slug {slug}"
                );
            }
            println!(
                "Rien n’a été supprimé. Pour la supprimer avec tout ce qu’elle garde, son \
                 journal d’audit compris, sans retour :\n  mimorya-admin delete-company \
                 --slug {slug} --confirm {slug}"
            );
        }
        CompanyDeletion::Deleted(rows) => {
            println!(
                "Entreprise de sigle {slug} supprimée (identifiant {}), avec ce qu’elle gardait :",
                rows.tenant_id
            );
            println!("{}", held(&rows));
            outside(&rows);
            println!(
                "Les sauvegardes faites avant la gardent jusqu’à leur retrait : après une \
                 restauration depuis l’une d’elles, supprimez-la de nouveau (gardez cette \
                 sortie). Ses ancres du journal d’audit ne sont plus à donner à verify-audit."
            );
        }
    }
    Ok(())
}

/// What the company declared at other services, said when there is any:
/// before the release, what it withdraws there is among it; after, what
/// stays there is to withdraw there.
fn outside(rows: &CompanyRows) {
    let held = rows.held_outside();
    if held.is_empty() {
        return;
    }
    if rows.to_release > 0 {
        println!(
            "Déclarés chez leurs fournisseurs (release-company y retire les autorisations OAuth \
             et les bots Telegram ; le reste est à retirer là) :"
        );
    } else {
        println!("Restent déclarés chez leurs fournisseurs, à retirer là :");
    }
    for (what, count) in held {
        println!("  {what} : {count}");
    }
}

/// What `rows` holds, a table a line, then the total.
fn held(rows: &CompanyRows) -> String {
    let mut lines: Vec<String> = rows
        .tables
        .iter()
        .map(|(table, count)| format!("  {table} : {count}"))
        .collect();
    lines.push(format!(
        "  soit {} lignes dans {} tables",
        rows.total(),
        rows.tables.len()
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn says_each_table_then_the_total() {
        let rows = CompanyRows {
            tenant_id: Uuid::nil(),
            tables: vec![("audit_log".into(), 12), ("users".into(), 3)],
            to_release: 0,
        };
        assert_eq!(
            held(&rows),
            "  audit_log : 12\n  users : 3\n  soit 15 lignes dans 2 tables"
        );
    }
}
