//! The company's audit log as its SIEM reads it with a token (ADR-181),
//! before any session: the token names the company, an unknown one and a
//! suspended company's answer alike (401). The sealed entries after the
//! place asked, in the chain's order, each with its place and its seal, so
//! that the SIEM resumes after the last place it holds and keeps seals as
//! anchors to give the company's verification (ADR-179). An entry not yet
//! sealed is read once it is, within the minute. Reading is noted on the
//! token (when it was last used), not in the audit log, which each read
//! would otherwise grow. A server without a master key seals nothing, and
//! exports nothing.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::sealed_audit_entries;
use mimorya_db::repositories::audit_export_tokens::{
    audit_export_token_tenant, touch_audit_export_token,
};
use uuid::Uuid;

use crate::auth::credentials::hash_token;
use crate::config::AppConfig;
use crate::contract::{AuditExport, ExportedAuditEntry};

/// Entries a page holds when the SIEM does not say.
pub const DEFAULT_PAGE: i64 = 500;
/// The most entries a page holds.
pub const MAX_PAGE: i64 = 1000;

const UNKNOWN: &str = "Jeton d’export du journal d’audit invalide.";
const UNSEALED: &str = "Ce serveur ne scelle pas le journal d’audit : aucune clé maître \
     (MIMORYA_MASTER_KEY) n’y est configurée, rien n’en est exporté. Adressez-vous à \
     l’exploitant de la plateforme.";

/// What the SIEM asks: the entries after the place `after`, at most `limit`.
#[derive(Debug, Clone, Copy)]
pub struct ExportPage {
    pub after: i64,
    pub limit: i64,
}

/// The page of the audit log the token `token` reads.
pub async fn export_audit_log(
    db: &Database,
    config: &AppConfig,
    token: Option<&str>,
    page: ExportPage,
    now: DateTime<Utc>,
) -> anyhow::Result<AuditExport> {
    let (tenant_id, token_id) = token_company(db, token).await?;
    if config.secrets.is_none() {
        return Err(DomainError::unavailable(UNSEALED).into());
    }
    let limit = page.limit.clamp(1, MAX_PAGE);
    let mut tx = db.begin(tenant_id).await?;
    let entries = sealed_audit_entries(&mut tx, (page.after.max(0), i64::MAX), limit).await?;
    touch_audit_export_token(&mut tx, token_id, now).await?;
    tx.commit().await?;
    let more = entries.len() as i64 == limit;
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
