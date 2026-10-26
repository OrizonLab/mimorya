//! A person's own inbound hooks (ADR-109): listed, created, changed,
//! paused, given a new token and deleted by their author only; anyone
//! else's answer as absent. Creating or resuming one needs the right to use
//! the agent; at most `MAX_HOOKS` each. The token is shown once, when made:
//! only its digest is kept, never logged nor audited. Each change is
//! audited, never the instruction's text.

use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::inbound_hooks::{
    HookDefinition, InboundHookRow, count_inbound_hooks, create_inbound_hook, delete_inbound_hook,
    list_inbound_hooks, lock_inbound_hook, replace_hook_token, update_inbound_hook,
};
use serde_json::json;
use uuid::Uuid;

use crate::auth::credentials::{hash_token, new_session_token};
use crate::services::AccessContext;
use crate::services::scheduled_tasks::checked_request;

/// The hooks one person may keep.
pub const MAX_HOOKS: i64 = 10;
/// What a hook's token starts with: told apart from any other secret.
pub const TOKEN_PREFIX: &str = "mhk_";
const PERMISSION: &str = "agent:use";

const UNKNOWN_HOOK: &str = "Déclencheur introuvable";
const TOO_MANY: &str = "Vous avez déjà 10 déclencheurs : supprimez-en un d’abord.";

/// A hook as written: what it asks, and whether it receives.
pub struct HookInput {
    pub title: String,
    pub instruction: String,
    pub enabled: bool,
}

/// What changes in a hook; what is absent stays.
#[derive(Default)]
pub struct HookChanges {
    pub title: Option<String>,
    pub instruction: Option<String>,
    pub enabled: Option<bool>,
}

/// A token to give the system that posts, and its digest, kept alone.
fn new_token() -> (String, String) {
    let (random, _) = new_session_token();
    let token = format!("{TOKEN_PREFIX}{random}");
    let digest = hash_token(&token);
    (token, digest)
}

pub async fn inbound_hooks(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<Vec<InboundHookRow>> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let hooks = list_inbound_hooks(&mut tx, access.subject.user_id).await?;
    tx.commit().await?;
    Ok(hooks)
}

/// The new hook's id, and its token, shown this once.
pub async fn create_hook(
    db: &Database,
    access: &AccessContext,
    input: HookInput,
) -> anyhow::Result<(Uuid, String)> {
    access.require(PERMISSION)?;
    let (title, instruction) = checked_request(&input.title, &input.instruction)?;
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    if count_inbound_hooks(&mut tx, user_id).await? >= MAX_HOOKS {
        return Err(DomainError::conflict(TOO_MANY).into());
    }
    let (token, digest) = new_token();
    let definition = HookDefinition {
        title: &title,
        instruction: &instruction,
        enabled: input.enabled,
    };
    let id = create_inbound_hook(&mut tx, tenant_id, user_id, definition, &digest).await?;
    append_audit(
        &mut tx,
        audited(access, "inbound_hook.created", id).metadata(json!({ "enabled": input.enabled })),
    )
    .await?;
    tx.commit().await?;
    Ok((id, token))
}

pub async fn change_hook(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
    changes: HookChanges,
) -> anyhow::Result<()> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let Some(current) = lock_inbound_hook(&mut tx, id, access.subject.user_id).await? else {
        return Err(DomainError::not_found(UNKNOWN_HOOK).into());
    };
    let enabled = changes.enabled.unwrap_or(current.enabled);
    // Pausing needs no right: one who lost the agent may still stop theirs.
    if enabled {
        access.require(PERMISSION)?;
    }
    let (title, instruction) = checked_request(
        changes.title.as_deref().unwrap_or(&current.title),
        changes
            .instruction
            .as_deref()
            .unwrap_or(&current.instruction),
    )?;
    let definition = HookDefinition {
        title: &title,
        instruction: &instruction,
        enabled,
    };
    update_inbound_hook(&mut tx, id, definition).await?;
    append_audit(
        &mut tx,
        audited(access, "inbound_hook.updated", id).metadata(json!({ "enabled": enabled })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// A new token for the hook `id`, shown this once: the former one no
/// longer delivers.
pub async fn replace_token(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<String> {
    access.require(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    if lock_inbound_hook(&mut tx, id, access.subject.user_id)
        .await?
        .is_none()
    {
        return Err(DomainError::not_found(UNKNOWN_HOOK).into());
    }
    let (token, digest) = new_token();
    replace_hook_token(&mut tx, id, &digest).await?;
    append_audit(&mut tx, audited(access, "inbound_hook.token_replaced", id)).await?;
    tx.commit().await?;
    Ok(token)
}

pub async fn delete_hook(db: &Database, access: &AccessContext, id: Uuid) -> anyhow::Result<()> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    if !delete_inbound_hook(&mut tx, id, access.subject.user_id).await? {
        return Err(DomainError::not_found(UNKNOWN_HOOK).into());
    }
    append_audit(&mut tx, audited(access, "inbound_hook.deleted", id)).await?;
    tx.commit().await?;
    Ok(())
}

/// A change of the hook `id` by its author: never what it asks, nor its token.
fn audited(access: &AccessContext, action: &str, id: Uuid) -> AuditEntry {
    AuditEntry::new(access.subject.tenant_id, "user", action, "success")
        .by_user(access.subject.user_id)
        .resource("inbound_hook", id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_told_apart_and_only_its_digest_is_kept() {
        let (token, digest) = new_token();
        assert!(token.starts_with(TOKEN_PREFIX));
        assert_eq!(token.len(), TOKEN_PREFIX.len() + 43);
        assert_eq!(digest, hash_token(&token));
        assert_ne!(token, new_token().0);
    }
}
