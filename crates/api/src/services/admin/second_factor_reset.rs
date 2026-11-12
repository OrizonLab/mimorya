//! An account's second factor reset by an administrator (ADR-202): for who
//! lost its app and its recovery codes, or to change apps where the company
//! requires one. Who manages accounts company-wide (`user:manage`) resets
//! another's, never its own, as a password. The second factor and its
//! recovery codes go, the sign-ins waiting for a code end, every session of
//! the account closes; its next sign-in with a password opens a session, or
//! first an enrolment where the company requires one. Audited.

use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::auth::lock_login_state;
use mimorya_db::repositories::login_challenges::end_login_challenges;
use mimorya_db::repositories::second_factors::delete_second_factor;
use mimorya_db::repositories::users::revoke_user_sessions;
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change, refuse_self};
use crate::services::AccessContext;

const PERMISSION: &str = "user:manage";
const NONE_ON: &str = "Ce compte n’a pas de second facteur actif : rien à réinitialiser";

/// Resets the second factor of `user_id`.
pub async fn reset_second_factor(
    db: &Database,
    access: &AccessContext,
    user_id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    refuse_self(access, user_id)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    // The account's row first, as a sign-in or an activation locks it: a
    // sign-in crossing the reset either opened its session before, which
    // closes here, or reads no second factor.
    if lock_login_state(&mut tx, user_id).await?.is_none() {
        return Err(DomainError::not_found("Compte introuvable").into());
    }
    // An enrolment that waits is no second factor: nothing changes then.
    if !delete_second_factor(&mut tx, user_id).await? {
        return Err(DomainError::conflict(NONE_ON).into());
    }
    let challenges = end_login_challenges(&mut tx, user_id).await?;
    let sessions = revoke_user_sessions(&mut tx, user_id, None).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "user.second_factor_reset", "user", user_id).metadata(json!({
            "sessionsClosed": sessions,
            "signInsEnded": challenges,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
