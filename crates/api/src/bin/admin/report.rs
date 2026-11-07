//! What `mimorya-admin verify-audit` tells the operator of each company's
//! audit chain (ADR-178): entries verified, entries still to seal, the last
//! place and its seal, the breaks (missing places as ranges) and what each
//! anchor finds; and what it found wrong over all companies, which fails
//! the command.

use anyhow::bail;
use mimorya_api::services::audit_chain::{
    AnchorFound, ChainBreak, ChainReport, LISTED_BREAKS, hex,
};

use crate::anchor::Anchor;

/// What the verification found wrong, company by company.
#[derive(Debug, Default)]
pub struct Tally {
    pub broken: usize,
    pub unanchored: usize,
    pub failed: usize,
}

impl Tally {
    pub fn count(&mut self, report: &ChainReport) {
        self.broken += usize::from(report.broken > 0);
        let missed = report
            .anchors
            .iter()
            .filter(|found| **found != AnchorFound::Held);
        self.unanchored += missed.count();
    }

    /// Fails unless every chain holds, every anchor with it.
    pub fn finish(self) -> anyhow::Result<()> {
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
pub fn told(company: &str, report: &ChainReport, anchors: &[&Anchor]) -> String {
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
        for at in report.breaks.iter().take(LISTED_BREAKS) {
            line.push_str(&match at {
                ChainBreak::Missing { from, to } if from == to => {
                    format!(" n° {from} sans entrée (supprimée) ;")
                }
                ChainBreak::Missing { from, to } => {
                    format!(" n° {from} à {to} sans entrée (supprimées) ;")
                }
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
    use mimorya_db::repositories::audit_chain::ChainHead;

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
            ChainBreak::Missing { from: 3, to: 3 },
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
    fn tells_entries_removed_one_after_another_as_one_range() {
        let breaks = vec![ChainBreak::Missing { from: 5, to: 504 }];
        let mut removed = report(breaks, Vec::new());
        removed.broken = 500;
        let line = told("acme", &removed, &[]);
        assert!(line.contains("ROMPUE, 500 rupture(s) : n° 5 à 504 sans entrée (supprimées)"));
    }

    #[test]
    fn fails_unless_every_chain_holds_with_its_anchors() {
        let mut held = Tally::default();
        held.count(&report(Vec::new(), vec![AnchorFound::Held]));
        assert!(held.finish().is_ok());
        let mut broken = Tally::default();
        broken.count(&report(
            vec![ChainBreak::Missing { from: 1, to: 1 }],
            Vec::new(),
        ));
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
}
