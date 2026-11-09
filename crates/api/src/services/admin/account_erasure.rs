//! An administrator erases a disabled account (ADR-189): with the account
//! go what the platform keeps on the person, its conversations, memories,
//! tasks, triggers, missions, runs, validations, links and sessions, every
//! table naming it deleting its rows or emptying its column. Company-wide
//! and irreversible: its own permission (`user:erase`), never one's own
//! account, the account's address typed again, refused while it ties to
//! others (`account_erasure` in the core). The audit log keeps the entries
//! naming it, unchanged and sealed, and traces the erasure by its counts.

use mimorya_core::DomainError;
use mimorya_core::account_erasure::{ErasureCandidate, ErasureObstacle, obstacle};
use mimorya_db::repositories::account_erasure::{
    ErasedCounts, erase_account as delete_account, erasure_candidate,
};
use mimorya_db::repositories::audit::append_audit;
use serde_json::json;
use uuid::Uuid;

use super::account_access::{let_go, take_access};
use super::connectors::ConnectorAdmin;
use super::{audit_entry, begin_change, refuse_self};
use crate::services::AccessContext;

const PERMISSION: &str = "user:erase";
const NO_ACCOUNT: &str = "Compte introuvable";

/// Erases the disabled account `user_id`, whose address `typed` confirms;
/// what went, by its counts.
pub async fn erase_account(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    user_id: Uuid,
    typed: &str,
) -> anyhow::Result<ErasedCounts> {
    access.require_tenant_wide(PERMISSION)?;
    refuse_self(access, user_id)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    let row = erasure_candidate(&mut tx, user_id)
        .await?
        .ok_or_else(|| DomainError::not_found(NO_ACCOUNT))?;
    let candidate = ErasureCandidate {
        status: row.status,
        email: row.email,
        live_runs: row.live_runs,
        awaited_decisions: row.awaited_decisions,
        stand_ins: row.stand_ins,
        led_units: row.led_units,
    };
    if let Some(found) = obstacle(&candidate, typed) {
        return Err(match found {
            ErasureObstacle::NotConfirmed => DomainError::validation(found.message()),
            _ => DomainError::conflict(found.message()),
        }
        .into());
    }
    // Disabling took what the account held; an account disabled before
    // some of it existed is taken from again, its tokens revoked.
    let revocations = take_access(&mut tx, admin, tenant_id, user_id).await?;
    let erased = delete_account(&mut tx, user_id)
        .await?
        .ok_or_else(|| DomainError::not_found(NO_ACCOUNT))?;
    let counts = json!({
        "conversations": erased.conversations,
        "messages": erased.messages,
        "memories": erased.memories,
        "runs": erased.runs,
    });
    append_audit(
        &mut tx,
        audit_entry(access, "user.erased", "user", user_id).metadata(counts),
    )
    .await?;
    tx.commit().await?;
    let_go(
        admin,
        tenant_id,
        Some(access.subject.user_id),
        user_id,
        revocations,
    )
    .await;
    Ok(erased)
}
