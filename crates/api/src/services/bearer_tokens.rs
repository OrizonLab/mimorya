//! The tokens another system holds of a company (ADR-181, ADR-183), made,
//! listed and removed by who holds their kind's permission on the whole
//! company, at most `MAX_TOKENS` of a kind. A token is the company's, not
//! its maker's: it outlives their account, and names who made it. Its secret
//! is shown once, when made: only its digest is kept, never logged nor
//! audited; making and removing one are audited with its name. Then, before
//! any session, its company found by its secret, and its use noted.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::bearer_tokens::{
    BearerTokenRow, TokenTable, count_tokens, create_token, delete_token, list_tokens,
    token_tenant, touch_token,
};
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use crate::auth::credentials::{hash_token, new_session_token};
use crate::services::AccessContext;

/// The tokens of a kind a company may keep.
pub const MAX_TOKENS: i64 = 5;
/// The longest name a token takes, in characters.
pub const NAME_LENGTH: usize = 80;

/// A kind of token: where it is kept, who manages it, how it is told apart
/// and what is said of it.
pub struct TokenKind {
    pub table: TokenTable,
    /// Held on the whole company, it manages the kind's tokens.
    pub permission: &'static str,
    /// What a token starts with: told apart from any other secret.
    pub prefix: &'static str,
    /// The audited resource, and its two actions.
    pub resource: &'static str,
    pub created: &'static str,
    pub deleted: &'static str,
    /// Removing a token that is not there.
    pub unknown: &'static str,
    /// Making one more than `MAX_TOKENS`.
    pub too_many: fn() -> String,
    /// Making one without a name, or with a name too long.
    pub no_name: fn() -> String,
}

impl TokenKind {
    /// A token's secret, and its digest, kept alone.
    fn new_secret(&self) -> (String, String) {
        let (random, _) = new_session_token();
        let secret = format!("{}{random}", self.prefix);
        let digest = hash_token(&secret);
        (secret, digest)
    }

    /// The company's tokens of this kind, never their secret.
    pub async fn list(
        &self,
        db: &Database,
        access: &AccessContext,
    ) -> anyhow::Result<Vec<BearerTokenRow>> {
        access.require_tenant_wide(self.permission)?;
        let mut tx = db.begin(access.subject.tenant_id).await?;
        let tokens = list_tokens(&mut tx, self.table).await?;
        tx.commit().await?;
        Ok(tokens)
    }

    /// A new token named `name`, and its secret, shown this once.
    pub async fn create(
        &self,
        db: &Database,
        access: &AccessContext,
        name: &str,
    ) -> anyhow::Result<(BearerTokenRow, String)> {
        access.require_tenant_wide(self.permission)?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > NAME_LENGTH || name.contains('\0') {
            return Err(DomainError::validation((self.no_name)()).into());
        }
        let tenant_id = access.subject.tenant_id;
        let mut tx = db.begin(tenant_id).await?;
        // Two makings at once count each other.
        tx.lock_administration().await?;
        if count_tokens(&mut tx, self.table).await? >= MAX_TOKENS {
            return Err(DomainError::conflict((self.too_many)()).into());
        }
        let (secret, digest) = self.new_secret();
        let token = create_token(
            &mut tx,
            self.table,
            tenant_id,
            name,
            &digest,
            access.subject.user_id,
        )
        .await?;
        append_audit(&mut tx, self.audited(access, self.created, &token)).await?;
        tx.commit().await?;
        Ok((token, secret))
    }

    /// Removes the company's token `id`: its holder reaches nothing more.
    pub async fn delete(
        &self,
        db: &Database,
        access: &AccessContext,
        id: Uuid,
    ) -> anyhow::Result<()> {
        access.require_tenant_wide(self.permission)?;
        let mut tx = db.begin(access.subject.tenant_id).await?;
        // The token's row first, then the administration's lock: the order
        // a request with the token takes them in (its use noted, then a
        // change it makes), so that neither waits on the other in a cycle.
        let Some(token) = delete_token(&mut tx, self.table, id).await? else {
            return Err(DomainError::not_found(self.unknown).into());
        };
        tx.lock_administration().await?;
        append_audit(&mut tx, self.audited(access, self.deleted, &token)).await?;
        tx.commit().await?;
        Ok(())
    }

    /// The company and the token `secret` names, before any session; `None`
    /// alike when there is none, it is unknown or its company is suspended.
    pub async fn company(
        &self,
        db: &Database,
        secret: Option<&str>,
    ) -> anyhow::Result<Option<(Uuid, Uuid)>> {
        match secret {
            Some(secret) => Ok(token_tenant(db.pool(), self.table, &hash_token(secret)).await?),
            None => Ok(None),
        }
    }

    /// Notes in the company's transaction that its token `id` was used at
    /// `now`; whether it is still there. Removed meanwhile, it reaches
    /// nothing.
    pub async fn touch(
        &self,
        tx: &mut TenantTx,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        Ok(touch_token(tx, self.table, id, now).await?)
    }

    /// The audit entry of a change to `token`: its name, never its secret.
    fn audited(&self, access: &AccessContext, action: &str, token: &BearerTokenRow) -> AuditEntry {
        AuditEntry::new(access.subject.tenant_id, "user", action, "success")
            .by_user(access.subject.user_id)
            .resource(self.resource, token.id)
            .metadata(json!({ "name": token.name }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIND: TokenKind = TokenKind {
        table: TokenTable::Scim,
        permission: "user:manage",
        prefix: "tst_",
        resource: "test_token",
        created: "test.token_created",
        deleted: "test.token_deleted",
        unknown: "Jeton introuvable",
        too_many: || String::new(),
        no_name: || String::new(),
    };

    #[test]
    fn a_token_is_prefixed_and_only_its_digest_kept() {
        let (secret, digest) = KIND.new_secret();
        assert!(secret.starts_with(KIND.prefix));
        assert!(secret.len() > KIND.prefix.len() + 40, "{secret}");
        assert_eq!(digest, hash_token(&secret));
        assert_eq!(digest.len(), 64);
        assert!(!digest.contains(&secret[KIND.prefix.len()..]));
        assert_ne!(KIND.new_secret().0, secret);
    }
}
