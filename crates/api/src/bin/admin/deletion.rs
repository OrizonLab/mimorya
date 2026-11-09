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
            println!(
                "Les sauvegardes faites avant la gardent jusqu’à leur retrait ; restaurée \
                 depuis l’une d’elles, elle reviendrait. Ses ancres du journal d’audit ne sont \
                 plus à donner à verify-audit."
            );
        }
    }
    Ok(())
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
        };
        assert_eq!(
            held(&rows),
            "  audit_log : 12\n  users : 3\n  soit 15 lignes dans 2 tables"
        );
    }
}
