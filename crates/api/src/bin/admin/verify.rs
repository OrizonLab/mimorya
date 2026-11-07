//! `mimorya-admin verify-audit` (ADR-178): each company's audit chain
//! verified from its first place, under the key derived from the master
//! key, and against the anchors given (seals logged by the sealing, as
//! `<company>:<place>:<seal>`, the company by its identifier as logged or
//! by its slug, which alone show entries removed from a chain's end or
//! seals set again), then told to the operator: entries verified, entries
//! still to seal, the last place and its seal, each break and each anchor.
//! A company named by an anchor is verified even if no entry is left in its
//! log. A company that cannot be verified is told so and the others are
//! verified still. Fails if a chain breaks, an anchor is not found, or a
//! company could not be verified.

use anyhow::anyhow;
use mimorya_api::AppConfig;
use mimorya_api::services::audit_chain::{Anchor, ChainKey, verify_company};
use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{ChainHead, audit_log_tenants};
use mimorya_db::repositories::tenants::tenant_slug;
use uuid::Uuid;

use crate::report::{Tally, told};

pub async fn verify_audit(
    config: &AppConfig,
    db: &Database,
    anchors: &[Anchor],
) -> anyhow::Result<()> {
    let key = chain_key(config)?;
    let mut tally = Tally::default();
    let mut used = vec![false; anchors.len()];
    let companies = to_verify(audit_log_tenants(db.pool(), false).await?, anchors);
    for tenant_id in &companies {
        // Named by its identifier, an anchor is the company's, read or not.
        for index in anchors_of(anchors, *tenant_id, "") {
            used[index] = true;
        }
        let company = match company(db, *tenant_id).await {
            Ok(company) => company,
            Err(error) => {
                println!("{tenant_id} : vérification impossible : {error:#}");
                tally.failed += 1;
                continue;
            }
        };
        let theirs = anchors_of(anchors, *tenant_id, &company);
        theirs.iter().for_each(|index| used[*index] = true);
        let theirs: Vec<&Anchor> = theirs.iter().map(|index| &anchors[*index]).collect();
        let heads: Vec<ChainHead> = theirs.iter().map(|anchor| anchor.head()).collect();
        match verify_company(db, &key, *tenant_id, &heads).await {
            Ok(report) => {
                println!("{}", told(&company, &report, &theirs));
                tally.count(&report);
            }
            Err(error) => {
                println!("{company} : vérification impossible : {error:#}");
                tally.failed += 1;
            }
        }
    }
    for (anchor, _) in anchors.iter().zip(&used).filter(|(_, used)| !**used) {
        println!(
            "Ancre n° {} de « {} » : aucune entreprise vérifiée ne porte ce sigle ; si toutes \
             ses entrées ont été supprimées, donnez son identifiant, tel que journalisé",
            anchor.seq, anchor.company
        );
        tally.unanchored += 1;
    }
    if companies.is_empty() {
        println!("Aucune entreprise n’a encore d’entrée dans son journal d’audit.");
    }
    tally.finish()
}

/// The companies to verify: those whose audit log has entries, and those
/// an anchor names by their identifier, whatever is left of their log.
fn to_verify(mut companies: Vec<Uuid>, anchors: &[Anchor]) -> Vec<Uuid> {
    for named in anchors
        .iter()
        .filter_map(|a| Uuid::parse_str(&a.company).ok())
    {
        if !companies.contains(&named) {
            companies.push(named);
        }
    }
    companies
}

/// The anchors of the company `tenant_id`, named by its identifier or by
/// its slug (none by an empty one): their places in `anchors`.
fn anchors_of(anchors: &[Anchor], tenant_id: Uuid, slug: &str) -> Vec<usize> {
    (0..anchors.len())
        .filter(|index| anchors[*index].names(tenant_id, slug))
        .collect()
}

/// The key the audit log is sealed under, which only the master key gives.
fn chain_key(config: &AppConfig) -> anyhow::Result<ChainKey> {
    let cipher = config.secrets.as_deref().ok_or_else(|| {
        anyhow!(
            "MIMORYA_MASTER_KEY est obligatoire : le journal d’audit est scellé sous une clé \
             qui en dérive"
        )
    })?;
    ChainKey::of(cipher)
}

async fn company(db: &Database, tenant_id: Uuid) -> anyhow::Result<String> {
    let mut tx = db.begin(tenant_id).await?;
    let slug = tenant_slug(&mut tx, tenant_id).await?;
    Ok(slug.unwrap_or_else(|| tenant_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_name_a_company_by_its_identifier_or_its_slug() {
        let (acme, other) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let named = |company: String| Anchor {
            company,
            seq: 1,
            seal: vec![0xcd; 32],
        };
        let anchors = [
            named(acme.to_string()),
            named("acme".into()),
            named(other.to_string()),
            named("beta".into()),
        ];
        assert_eq!(anchors_of(&anchors, acme, "acme"), [0, 1]);
        assert_eq!(anchors_of(&anchors, acme, ""), [0]);
        assert_eq!(anchors_of(&anchors, other, "autre"), [2]);
        // A company an anchor names by its identifier is verified, even
        // without an entry left; a slug alone names none.
        assert_eq!(to_verify(vec![acme], &anchors), [acme, other]);
        assert_eq!(to_verify(vec![acme, other], &anchors), [acme, other]);
    }

    #[test]
    fn refuses_to_verify_without_the_master_key() {
        let vars = [("DATABASE_APP_URL", "postgres://app:app@localhost/mimorya")];
        let config = AppConfig::from_vars(|key| {
            vars.iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_string())
        })
        .unwrap();
        let refused = chain_key(&config).err().map(|error| error.to_string());
        assert!(refused.is_some_and(|message| message.contains("MIMORYA_MASTER_KEY")));
    }
}
