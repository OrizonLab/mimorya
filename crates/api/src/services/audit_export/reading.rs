//! The company's audit log as its SIEM reads it with a token (ADR-181),
//! before any session: the token names the company, an unknown one and a
//! suspended company's answer alike (401). The sealed entries after the
//! place asked, in the chain's order, each with its place and its seal, so
//! that the SIEM resumes after the last place it holds and keeps seals as
//! anchors to give the company's verification (ADR-179). An entry not yet
//! sealed is read once it is, within the minute. Past the start, the SIEM
//! gives the seal of the place it resumes after, which each exported entry
//! carries (`Resume`): a place the chain no longer counts, or whose seal is
//! no longer the one given, is refused (409) with the chain's last place
//! (`lastPlace` in its details), the token's use noted all the same. The chain
//! was restored behind the SIEM, which resumes from a place it still shares
//! with it rather than miss what was sealed again. Once the chain's oldest
//! places went, five years old (ADR-198), the place before its start (its
//! start holding its seal) is still shared, by the seal kept there; a place
//! before that is refused as gone with them (`firstPlace` in the details),
//! and the SIEM resumes from the start, the chain's first place now.
//! Reading is noted on the token (when it was last used), not in the audit
//! log, which each read would otherwise grow. A server without a master key
//! seals nothing, and exports nothing.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{
    audit_chain_head, audit_seal_at, sealed_audit_entries,
};
use serde_json::json;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::contract::{AuditExport, ExportedAuditEntry};
use crate::services::audit_chain::{ChainKey, Place, Start, chain_start, hex};

use super::tokens::EXPORT_TOKENS;

/// Entries a page holds when the SIEM does not say.
pub const DEFAULT_PAGE: i64 = 500;
/// The most entries a page holds.
pub const MAX_PAGE: i64 = 1000;

const UNKNOWN: &str = "Jeton d’export du journal d’audit invalide.";
const UNSEALED: &str = "Ce serveur ne scelle pas le journal d’audit : aucune clé maître \
     (MIMORYA_MASTER_KEY) n’y est configurée, rien n’en est exporté. Adressez-vous à \
     l’exploitant de la plateforme.";

/// Where the SIEM resumes: from the start, or after a place it holds, with
/// its seal there in hexadecimal (any case). Past the start, the seal is
/// always given: a restore behind the SIEM is never missed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resume<'a> {
    Start,
    After { place: i64, seal: &'a str },
}

/// What the SIEM asks: the entries after where it resumes, at most `limit`.
#[derive(Debug, Clone, Copy)]
pub struct ExportPage<'a> {
    pub resume: Resume<'a>,
    pub limit: i64,
}

/// The page of the audit log the token `token` reads.
pub async fn export_audit_log(
    db: &Database,
    config: &AppConfig,
    token: Option<&str>,
    page: ExportPage<'_>,
    now: DateTime<Utc>,
) -> anyhow::Result<AuditExport> {
    let (tenant_id, token_id) = token_company(db, token).await?;
    let Some(cipher) = config.secrets.as_deref() else {
        return Err(DomainError::unavailable(UNSEALED).into());
    };
    let key = ChainKey::of(cipher)?;
    let limit = page.limit.clamp(1, MAX_PAGE);
    let mut tx = db.begin(tenant_id).await?;
    // Removed meanwhile, the token reads nothing.
    if !EXPORT_TOKENS.touch(&mut tx, token_id, now).await? {
        return Err(DomainError::unauthenticated(UNKNOWN).into());
    }
    let after = match page.resume {
        Resume::Start => 0,
        Resume::After { place, seal } => {
            let held = audit_seal_at(&mut tx, place).await?;
            // Not held: perhaps gone with the oldest places.
            let start = match held {
                Some(_) => None,
                None => chain_start(&mut tx, &key, tenant_id).await?,
            };
            if let Some(unshared) = unshared(held.as_deref(), start.as_ref(), place, seal) {
                let last = audit_chain_head(&mut tx)
                    .await?
                    .map_or(0, |head| head.chain_seq);
                // Its use noted all the same: refused, the SIEM still reads.
                tx.commit().await?;
                return Err(unshared.refusal(place, last).into());
            }
            place
        }
    };
    // One more than the page: whether others follow.
    let mut entries = sealed_audit_entries(&mut tx, (after, i64::MAX), limit + 1).await?;
    tx.commit().await?;
    let more = entries.len() as i64 > limit;
    entries.truncate(limit as usize);
    Ok(AuditExport {
        company: tenant_id,
        entries: entries
            .iter()
            .map(ExportedAuditEntry::from_chain)
            .collect::<anyhow::Result<_>>()?,
        more,
    })
}

/// The company and the token `token` names; refused alike when there is
/// none, it is unknown or its company is suspended.
async fn token_company(db: &Database, token: Option<&str>) -> anyhow::Result<(Uuid, Uuid)> {
    let found = EXPORT_TOKENS.company(db, token).await?;
    found.ok_or_else(|| DomainError::unauthenticated(UNKNOWN).into())
}

/// How the place the SIEM resumes after is no longer the chain's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unshared {
    /// The chain no longer counts it.
    Gone,
    /// It holds another seal there.
    Changed,
    /// It went with the oldest places: the chain starts at `first`.
    Purged { first: i64 },
}

/// Whether the chain, holding `held` at the place `place` the SIEM resumes
/// after, or starting at `start`, still shares it with the SIEM, which
/// gives `given` (any case).
fn unshared(
    held: Option<&[u8]>,
    start: Option<&Start>,
    place: i64,
    given: &str,
) -> Option<Unshared> {
    let same = |seal: &[u8]| given.eq_ignore_ascii_case(&hex(seal));
    if let Some(held) = held {
        return (!same(held)).then_some(Unshared::Changed);
    }
    let Some(start) = start else {
        return Some(Unshared::Gone);
    };
    match start.place(place, same) {
        Place::Eve { held: true } => None,
        Place::Eve { held: false } => Some(Unshared::Changed),
        Place::Purged => Some(Unshared::Purged {
            first: start.first_seq,
        }),
        Place::InChain => Some(Unshared::Gone),
    }
}

impl Unshared {
    /// The refusal of a read after `after`, the chain counting `last` places,
    /// which its details give the SIEM (`lastPlace`), and where it starts
    /// once its oldest places went (`firstPlace`).
    fn refusal(self, after: i64, last: i64) -> DomainError {
        let what = match self {
            Unshared::Gone => format!("ne compte plus la place {after}"),
            Unshared::Changed => format!("ne porte plus ce sceau à la place {after}"),
            Unshared::Purged { first } => {
                return DomainError::conflict(format!(
                    "Le journal d’audit ne garde plus la place {after} : ses entrées de plus de \
                     cinq ans ont été supprimées et il commence aujourd’hui à la place {first}. \
                     Reprenez depuis son début."
                ))
                .with_details(json!({ "lastPlace": last, "firstPlace": first }));
            }
        };
        DomainError::conflict(format!(
            "Le journal d’audit {what} : il a été restauré en deçà et compte aujourd’hui \
             {last} places scellées. Reprenez depuis une place que vous détenez et dont il \
             porte encore le sceau."
        ))
        .with_details(json!({ "lastPlace": last }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_the_chain_no_longer_shares_is_refused_with_its_last_place() {
        let seal = [0xab_u8; 32];
        let given = "ab".repeat(32);
        assert_eq!(unshared(Some(&seal), None, 4, &given), None);
        assert_eq!(unshared(Some(&seal), None, 4, &"AB".repeat(32)), None);
        let changed = unshared(Some(&seal), None, 4, &"cd".repeat(32));
        assert_eq!(changed, Some(Unshared::Changed));
        assert_eq!(unshared(None, None, 9, &given), Some(Unshared::Gone));
        let refusal = Unshared::Changed.refusal(4, 3);
        assert_eq!(refusal.details, Some(json!({ "lastPlace": 3 })));
        let refused = refusal.message;
        assert!(
            refused.contains("ne porte plus ce sceau à la place 4"),
            "{refused}"
        );
        assert!(
            refused.contains("compte aujourd’hui 3 places scellées"),
            "{refused}"
        );
        let gone = Unshared::Gone.refusal(9, 3).message;
        assert!(gone.contains("ne compte plus la place 9"), "{gone}");
    }

    #[test]
    fn past_a_purge_the_eve_of_the_start_is_shared_an_earlier_place_gone_with_it() {
        let start = Start {
            first_seq: 10,
            previous_hash: vec![0xab; 32],
            purged_entries: 9,
        };
        let kept = "AB".repeat(32);
        assert_eq!(unshared(None, Some(&start), 9, &kept), None);
        let other = "cd".repeat(32);
        assert_eq!(
            unshared(None, Some(&start), 9, &other),
            Some(Unshared::Changed)
        );
        let purged = unshared(None, Some(&start), 4, &kept);
        assert_eq!(purged, Some(Unshared::Purged { first: 10 }));
        // A place after the start the chain does not hold: gone as before.
        assert_eq!(
            unshared(None, Some(&start), 12, &kept),
            Some(Unshared::Gone)
        );
        let refusal = Unshared::Purged { first: 10 }.refusal(4, 30);
        assert_eq!(
            refusal.details,
            Some(json!({ "lastPlace": 30, "firstPlace": 10 }))
        );
        let refused = refusal.message;
        assert!(refused.contains("ne garde plus la place 4"), "{refused}");
        assert!(
            refused.contains("commence aujourd’hui à la place 10"),
            "{refused}"
        );
    }
}
