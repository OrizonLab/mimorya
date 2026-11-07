//! The company's audit log as its SIEM reads it with a token (ADR-181),
//! before any session: the token names the company, an unknown one and a
//! suspended company's answer alike (401). The sealed entries after the
//! place asked, in the chain's order, each with its place and its seal, so
//! that the SIEM resumes after the last place it holds and keeps seals as
//! anchors to give the company's verification (ADR-179). An entry not yet
//! sealed is read once it is, within the minute. A place the chain no longer
//! counts, or whose seal is no longer the one the SIEM holds, is refused
//! (409): the chain was restored behind it, and the SIEM resumes from a
//! place it still shares with it rather than miss what was sealed again.
//! Reading is noted on the token (when it was last used), not in the audit
//! log, which each read would otherwise grow. A server without a master key
//! seals nothing, and exports nothing.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{audit_seal_at, sealed_audit_entries};
use mimorya_db::repositories::audit_export_tokens::{
    audit_export_token_tenant, touch_audit_export_token,
};
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

/// What the SIEM asks: the entries after the place `after`, at most `limit`;
/// the seal it holds at that place, if it gives it, in hexadecimal.
#[derive(Debug, Clone, Copy)]
pub struct ExportPage<'a> {
    pub after: i64,
    pub limit: i64,
    pub seal: Option<&'a str>,
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
    let after = page.after.max(0);
    let mut tx = db.begin(tenant_id).await?;
    // Removed meanwhile, the token reads nothing.
    if !touch_audit_export_token(&mut tx, token_id, now).await? {
        return Err(DomainError::unauthenticated(UNKNOWN).into());
    }
    if after > 0 {
        let held = audit_seal_at(&mut tx, after).await?;
        shared_place(after, held.as_deref(), page.seal)?;
    }
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

/// Whether the place `after` the SIEM holds is still the chain's: the chain
/// holds `held` there, and the SIEM, if it says, `given`.
fn shared_place(after: i64, held: Option<&[u8]>, given: Option<&str>) -> Result<(), DomainError> {
    let Some(held) = held else {
        return Err(DomainError::conflict(format!(
            "Le journal d’audit ne compte plus la place {after} : il a été restauré en deçà. \
             Reprenez depuis une place que vous détenez et qu’il porte encore."
        )));
    };
    match given {
        Some(given) if !given.eq_ignore_ascii_case(&hex(held)) => {
            Err(DomainError::conflict(format!(
                "Le journal d’audit ne porte plus ce sceau à la place {after} : il a été \
                 restauré en deçà. Reprenez depuis une place dont il porte encore le sceau."
            )))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_the_chain_no_longer_shares_is_refused() {
        let seal = [0xab_u8; 32];
        assert!(shared_place(4, Some(&seal), None).is_ok());
        assert!(shared_place(4, Some(&seal), Some(&"ab".repeat(32))).is_ok());
        assert!(shared_place(4, Some(&seal), Some(&"AB".repeat(32))).is_ok());
        let changed = shared_place(4, Some(&seal), Some(&"cd".repeat(32))).unwrap_err();
        assert!(
            changed
                .message
                .contains("ne porte plus ce sceau à la place 4")
        );
        let gone = shared_place(4, None, None).unwrap_err();
        assert!(gone.message.contains("ne compte plus la place 4"));
        assert!(shared_place(4, None, Some(&"ab".repeat(32))).is_err());
    }
}
