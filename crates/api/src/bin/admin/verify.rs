//! `mimorya-admin verify-audit` (ADR-178): each company's audit chain
//! verified from its first place, under the key derived from the master
//! key, and told to the operator: entries verified, entries still to seal,
//! the last place and its seal (to compare with those the sealing logged:
//! entries removed from the chain's end show only there), or where the
//! chain breaks. Fails if one breaks.

use anyhow::{anyhow, bail};
use mimorya_api::AppConfig;
use mimorya_api::services::audit_chain::{ChainBreak, ChainKey, ChainReport, hex, verify_company};
use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::audit_log_tenants;
use mimorya_db::repositories::tenants::tenant_slug;
use uuid::Uuid;

pub async fn verify_audit(config: &AppConfig, db: &Database) -> anyhow::Result<()> {
    let cipher = config.secrets.as_deref().ok_or_else(|| {
        anyhow!("MIMORYA_MASTER_KEY est obligatoire : le journal d’audit est scellé sous une clé qui en dérive")
    })?;
    let key = ChainKey::of(cipher)?;
    let mut broken = 0;
    let companies = audit_log_tenants(db.pool(), false).await?;
    for tenant_id in &companies {
        let report = verify_company(db, &key, *tenant_id).await?;
        println!("{}", told(&company(db, *tenant_id).await?, &report));
        broken += usize::from(report.broken.is_some());
    }
    if companies.is_empty() {
        println!("Aucune entreprise n’a encore d’entrée dans son journal d’audit.");
    }
    if broken > 0 {
        bail!("journal d’audit rompu dans {broken} entreprise(s)");
    }
    Ok(())
}

async fn company(db: &Database, tenant_id: Uuid) -> anyhow::Result<String> {
    let mut tx = db.begin(tenant_id).await?;
    let slug = tenant_slug(&mut tx, tenant_id).await?;
    Ok(slug.unwrap_or_else(|| tenant_id.to_string()))
}

/// What the operator is told of a company's chain.
fn told(company: &str, report: &ChainReport) -> String {
    let mut line = format!(
        "{company} : {} entrée(s) vérifiée(s), {} à sceller",
        report.verified, report.unsealed
    );
    if let Some(head) = &report.head {
        line.push_str(&format!(
            " ; dernier sceau vérifié, n° {} : {}",
            head.chain_seq,
            hex(&head.chain_hash)
        ));
    }
    match report.broken {
        None => line.push_str(" ; chaîne intacte"),
        Some(ChainBreak::Missing { seq }) => {
            line.push_str(&format!(
                " ; ROMPUE : aucune entrée ne porte le n° {seq} (supprimée)"
            ));
        }
        Some(ChainBreak::Altered { seq, entry_id }) => line.push_str(&format!(
            " ; ROMPUE : l’entrée {entry_id}, n° {seq}, n’est plus celle qui a été scellée"
        )),
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use mimorya_db::repositories::audit_chain::ChainHead;

    fn report(broken: Option<ChainBreak>) -> ChainReport {
        ChainReport {
            verified: 2,
            unsealed: 1,
            head: Some(ChainHead {
                chain_seq: 2,
                chain_hash: vec![0xab; 32],
            }),
            broken,
        }
    }

    #[test]
    fn tells_where_a_chain_breaks() {
        let intact = told("acme", &report(None));
        assert!(intact.starts_with("acme : 2 entrée(s) vérifiée(s), 1 à sceller"));
        assert!(intact.contains(&format!("n° 2 : {}", "ab".repeat(32))));
        assert!(intact.ends_with("chaîne intacte"));
        let missing = told("acme", &report(Some(ChainBreak::Missing { seq: 3 })));
        assert!(missing.ends_with("ROMPUE : aucune entrée ne porte le n° 3 (supprimée)"));
        let altered = told(
            "acme",
            &report(Some(ChainBreak::Altered {
                seq: 3,
                entry_id: 17,
            })),
        );
        assert!(altered.contains("l’entrée 17, n° 3, n’est plus celle"));
    }
}
