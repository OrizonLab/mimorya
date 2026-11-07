//! The tokens with which the company's SIEM reads its audit log (ADR-181):
//! made, listed and removed by who may export the log on the whole company
//! (`audit:export`), at most `MAX_TOKENS`. A token is the company's, not its
//! maker's: it outlives their account, and names who made it. Its secret is
//! shown once, when made: only its digest is kept, never logged nor
//! audited; making and removing one are audited with its name.

use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::audit_export_tokens::{
    self as stored, AuditExportTokenRow, count_audit_export_tokens, list_audit_export_tokens,
};
use serde_json::json;
use uuid::Uuid;

use crate::auth::credentials::{hash_token, new_session_token};
use crate::services::AccessContext;

/// The tokens a company may keep.
pub const MAX_TOKENS: i64 = 5;
/// What a token starts with: told apart from any other secret.
pub const TOKEN_PREFIX: &str = "mae_";
const PERMISSION: &str = "audit:export";
const RESOURCE: &str = "audit_export_token";

const UNKNOWN_TOKEN: &str = "Jeton d’export introuvable";
const TOO_MANY: &str = "L’entreprise a déjà 5 jetons d’export : supprimez-en un d’abord.";
const NO_NAME: &str = "Nommez le jeton : le SIEM qui le détiendra, en 80 caractères au plus.";

/// A token's secret, and its digest, kept alone.
fn new_token() -> (String, String) {
    let (random, _) = new_session_token();
    let token = format!("{TOKEN_PREFIX}{random}");
    let digest = hash_token(&token);
    (token, digest)
}

/// The company's tokens, never their secret.
pub async fn audit_export_tokens(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<Vec<AuditExportTokenRow>> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let tokens = list_audit_export_tokens(&mut tx).await?;
    tx.commit().await?;
    Ok(tokens)
}

/// A new token named `name`, and its secret, shown this once.
pub async fn create_audit_export_token(
    db: &Database,
    access: &AccessContext,
    name: &str,
) -> anyhow::Result<(AuditExportTokenRow, String)> {
    access.require_tenant_wide(PERMISSION)?;
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.contains('\0') {
        return Err(DomainError::validation(NO_NAME).into());
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = db.begin(tenant_id).await?;
    // Two makings at once count each other.
    tx.lock_administration().await?;
    if count_audit_export_tokens(&mut tx).await? >= MAX_TOKENS {
        return Err(DomainError::conflict(TOO_MANY).into());
    }
    let (secret, digest) = new_token();
    let token = stored::create_audit_export_token(
        &mut tx,
        tenant_id,
        name,
        &digest,
        access.subject.user_id,
    )
    .await?;
    append_audit(
        &mut tx,
        audited(access, "audit.export_token_created", &token),
    )
    .await?;
    tx.commit().await?;
    Ok((token, secret))
}

/// Removes the company's token `id`: its SIEM reads nothing more with it.
pub async fn delete_audit_export_token(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    tx.lock_administration().await?;
    let Some(token) = stored::delete_audit_export_token(&mut tx, id).await? else {
        return Err(DomainError::not_found(UNKNOWN_TOKEN).into());
    };
    append_audit(
        &mut tx,
        audited(access, "audit.export_token_deleted", &token),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The audit entry of a change to `token`: its name, never its secret.
fn audited(access: &AccessContext, action: &str, token: &AuditExportTokenRow) -> AuditEntry {
    AuditEntry::new(access.subject.tenant_id, "user", action, "success")
        .by_user(access.subject.user_id)
        .resource(RESOURCE, token.id)
        .metadata(json!({ "name": token.name }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_prefixed_and_only_its_digest_kept() {
        let (secret, digest) = new_token();
        assert!(secret.starts_with(TOKEN_PREFIX));
        assert!(secret.len() > TOKEN_PREFIX.len() + 40, "{secret}");
        assert_eq!(digest, hash_token(&secret));
        assert_eq!(digest.len(), 64);
        assert!(!digest.contains(&secret[TOKEN_PREFIX.len()..]));
        assert_ne!(new_token().0, secret);
    }
}
