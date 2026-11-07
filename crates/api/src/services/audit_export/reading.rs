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
//! with it rather than miss what was sealed again.
//! Reading is noted on the token (when it was last used), not in the audit
//! log, which each read would otherwise grow. A server without a master key
//! seals nothing, and exports nothing.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{
    audit_chain_head, audit_seal_at, sealed_audit_entries,
};
use mimorya_db::repositories::audit_export_tokens::{
    audit_export_token_tenant, touch_audit_export_token,
};
use serde_json::json;
use uuid::Uuid;

use crate::auth::credentials::hash_token;
use crate::config::AppConfig;
use crate::contract::{AuditExport, ExportedAuditEntry};
use crate::services::audit_chain::hex;

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
    if config.secrets.is_none() {
        return Err(DomainError::unavailable(UNSEALED).into());
    }
    let limit = page.limit.clamp(1, MAX_PAGE);
    let mut tx = db.begin(tenant_id).await?;
    // Removed meanwhile, the token reads nothing.
    if !touch_audit_export_token(&mut tx, token_id, now).await? {
        return Err(DomainError::unauthenticated(UNKNOWN).into());
    }
    let after = match page.resume {
        Resume::Start => 0,
        Resume::After { place, seal } => {
            let held = audit_seal_at(&mut tx, place).await?;
            if let Some(unshared) = unshared(held.as_deref(), seal) {
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
    let found = match token {
        Some(token) => audit_export_token_tenant(db.pool(), &hash_token(token)).await?,
        None => None,
    };
    found.ok_or_else(|| DomainError::unauthenticated(UNKNOWN).into())
}

/// How the place the SIEM resumes after is no longer the chain's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unshared {
    /// The chain no longer counts it.
    Gone,
    /// It holds another seal there.
    Changed,
}

/// Whether the chain, holding `held` at the place the SIEM resumes after,
/// still shares it with the SIEM, which gives `given` (any case).
fn unshared(held: Option<&[u8]>, given: &str) -> Option<Unshared> {
    match held {
        None => Some(Unshared::Gone),
        Some(held) if !given.eq_ignore_ascii_case(&hex(held)) => Some(Unshared::Changed),
        Some(_) => None,
    }
}

impl Unshared {
    /// The refusal of a read after `after`, the chain counting `last` places,
    /// which its details give the SIEM (`lastPlace`).
    fn refusal(self, after: i64, last: i64) -> DomainError {
        let what = match self {
            Unshared::Gone => format!("ne compte plus la place {after}"),
            Unshared::Changed => format!("ne porte plus ce sceau à la place {after}"),
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
        assert_eq!(unshared(Some(&seal), &"ab".repeat(32)), None);
        assert_eq!(unshared(Some(&seal), &"AB".repeat(32)), None);
        let changed = unshared(Some(&seal), &"cd".repeat(32));
        assert_eq!(changed, Some(Unshared::Changed));
        assert_eq!(unshared(None, &"ab".repeat(32)), Some(Unshared::Gone));
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
}
