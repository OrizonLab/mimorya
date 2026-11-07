//! `mimorya-admin verify-audit` (ADR-178): each company's audit chain
//! verified from its first place, under the key derived from the master
//! key, and against the anchors given (seals logged by the sealing, which
//! alone show entries removed from a chain's end or seals set again), then
//! told to the operator: entries verified, entries still to seal, the last
//! place and its seal, each break and each anchor. A company that cannot be
//! verified is told so and the others are verified still. Fails if a chain
//! breaks, an anchor is not found, or a company could not be verified.

use anyhow::{anyhow, bail};
use mimorya_api::AppConfig;
use mimorya_api::services::audit_chain::{
    AnchorFound, ChainBreak, ChainKey, ChainReport, hex, verify_company,
};
use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{ChainHead, audit_log_tenants};
use mimorya_db::repositories::tenants::tenant_slug;
use uuid::Uuid;

use crate::anchor::Anchor;

/// Breaks told on a company's line; the others are counted.
const TOLD_BREAKS: usize = 5;

pub async fn verify_audit(
    config: &AppConfig,
    db: &Database,
    anchors: &[Anchor],
) -> anyhow::Result<()> {
    let key = chain_key(config)?;
    let mut tally = Tally::default();
    let mut verified = Vec::new();
    let companies = audit_log_tenants(db.pool(), false).await?;
    for tenant_id in &companies {
        let company = match company(db, *tenant_id).await {
            Ok(company) => company,
            Err(error) => {
                println!("{tenant_id} : vérification impossible : {error:#}");
                tally.failed += 1;
                continue;
            }
        };
        let theirs: Vec<&Anchor> = anchors.iter().filter(|a| a.company == company).collect();
        let heads: Vec<ChainHead> = theirs
            .iter()
            .map(|anchor| ChainHead {
                chain_seq: anchor.seq,
                chain_hash: anchor.seal.clone(),
            })
            .collect();
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
        verified.push(company);
    }
    for anchor in anchors.iter().filter(|a| !verified.contains(&a.company)) {
        println!(
            "Ancre n° {} de « {} » : aucune entreprise vérifiée ne porte ce sigle",
            anchor.seq, anchor.company
        );
        tally.unanchored += 1;
    }
    if companies.is_empty() {
        println!("Aucune entreprise n’a encore d’entrée dans son journal d’audit.");
    }
    tally.finish()
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

/// What the verification found wrong, company by company.
#[derive(Debug, Default)]
struct Tally {
    broken: usize,
    unanchored: usize,
    failed: usize,
}

impl Tally {
    fn count(&mut self, report: &ChainReport) {
        self.broken += usize::from(report.broken > 0);
        let missed = report
            .anchors
            .iter()
            .filter(|found| **found != AnchorFound::Held);
        self.unanchored += missed.count();
    }

    /// Fails unless every chain holds, every anchor with it.
    fn finish(self) -> anyhow::Result<()> {
        let Self {
            broken,
            unanchored,
            failed,
        } = self;
        if broken + unanchored + failed > 0 {
            bail!(
                "journal d’audit : {broken} chaîne(s) rompue(s), {unanchored} ancre(s) non \
                 retrouvée(s), {failed} entreprise(s) non vérifiée(s)"
            );
        }
        Ok(())
    }
}

/// What the operator is told of a company's chain and of its anchors.
fn told(company: &str, report: &ChainReport, anchors: &[&Anchor]) -> String {
    let mut line = format!(
        "{company} : {} entrée(s) vérifiée(s), {} à sceller",
        report.verified, report.unsealed
    );
    if let Some(head) = &report.head {
        line.push_str(&format!(
            " ; dernier sceau, n° {} : {}",
            head.chain_seq,
            hex(&head.chain_hash)
        ));
    }
    if report.broken == 0 {
        line.push_str(" ; chaîne intacte");
    } else {
        line.push_str(&format!(" ; ROMPUE, {} rupture(s) :", report.broken));
        for at in report.breaks.iter().take(TOLD_BREAKS) {
            line.push_str(&match at {
                ChainBreak::Missing { seq } => format!(" n° {seq} sans entrée (supprimée) ;"),
                ChainBreak::Altered { seq, entry_id } => {
                    format!(" entrée {entry_id}, n° {seq}, qui n’est plus celle scellée ;")
                }
            });
        }
    }
    for (anchor, found) in anchors.iter().zip(&report.anchors) {
        line.push_str(&format!(" ; ancre n° {} : ", anchor.seq));
        line.push_str(match found {
            AnchorFound::Held => "retrouvée",
            AnchorFound::Differs => "AUTRE SCEAU à cette place (chaîne scellée à nouveau)",
            AnchorFound::Missing => "ABSENTE (fin de chaîne retirée, ou sceaux retirés)",
        });
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(broken: Vec<ChainBreak>, anchors: Vec<AnchorFound>) -> ChainReport {
        ChainReport {
            verified: 2,
            unsealed: 1,
            head: Some(ChainHead {
                chain_seq: 2,
                chain_hash: vec![0xab; 32],
            }),
            broken: broken.len() as i64,
            breaks: broken,
            anchors,
        }
    }

    fn anchor(seq: i64) -> Anchor {
        Anchor {
            company: "acme".into(),
            seq,
            seal: vec![0xcd; 32],
        }
    }

    #[test]
    fn tells_where_a_chain_breaks_and_what_its_anchors_find() {
        let intact = told("acme", &report(Vec::new(), Vec::new()), &[]);
        assert!(intact.starts_with("acme : 2 entrée(s) vérifiée(s), 1 à sceller"));
        assert!(intact.contains(&format!("n° 2 : {}", "ab".repeat(32))));
        assert!(intact.ends_with("chaîne intacte"));
        let breaks = vec![
            ChainBreak::Missing { seq: 3 },
            ChainBreak::Altered {
                seq: 5,
                entry_id: 17,
            },
        ];
        let broken = told("acme", &report(breaks, Vec::new()), &[]);
        assert!(broken.contains("ROMPUE, 2 rupture(s) : n° 3 sans entrée (supprimée) ;"));
        assert!(broken.contains("entrée 17, n° 5, qui n’est plus celle scellée"));
        let (one, two, three) = (anchor(1), anchor(2), anchor(9));
        let found = vec![
            AnchorFound::Held,
            AnchorFound::Differs,
            AnchorFound::Missing,
        ];
        let anchored = told("acme", &report(Vec::new(), found), &[&one, &two, &three]);
        assert!(anchored.contains("ancre n° 1 : retrouvée"));
        assert!(anchored.contains("ancre n° 2 : AUTRE SCEAU"));
        assert!(anchored.contains("ancre n° 9 : ABSENTE"));
    }

    #[test]
    fn fails_unless_every_chain_holds_with_its_anchors() {
        let mut held = Tally::default();
        held.count(&report(Vec::new(), vec![AnchorFound::Held]));
        assert!(held.finish().is_ok());
        let mut broken = Tally::default();
        broken.count(&report(vec![ChainBreak::Missing { seq: 1 }], Vec::new()));
        assert!(broken.finish().is_err());
        for found in [AnchorFound::Differs, AnchorFound::Missing] {
            let mut unanchored = Tally::default();
            unanchored.count(&report(Vec::new(), vec![AnchorFound::Held, found]));
            let error = unanchored.finish().unwrap_err().to_string();
            assert!(error.contains("1 ancre(s) non retrouvée(s)"), "{error}");
        }
        let failed = Tally {
            failed: 1,
            ..Tally::default()
        };
        assert!(failed.finish().is_err());
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
