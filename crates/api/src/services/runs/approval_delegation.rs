//! Declaring an absence (ADR-071): whoever decides requests as a manager
//! names a colleague who stands in for them, for a period of at most 90
//! days. Meanwhile, the delegate may decide the pending requests routed to
//! them, where the delegate holds `approval:decide` over whom each is for:
//! a delegation grants no right. One per person, replaced when declared
//! again, withdrawn at will; each change is audited. An account
//! administrator may declare it for a collaborator who cannot (ADR-078,
//! `services/admin/absences.rs`), with the same checks.

use chrono::{DateTime, Duration, Utc};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::approval_delegations::{
    ApprovalDelegation, Colleague, NewDelegation, delegations_to, delete_delegation,
    get_delegation, possible_stand_ins, set_delegation,
};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::users::{DirectoryEntry, get_user};
use serde_json::json;
use sqlx::PgConnection;

use crate::services::access::AuthIdentity;

/// The longest absence one declaration covers.
pub const MAX_ABSENCE: Duration = Duration::days(90);
/// Ninety days from the start of one to the end of the last, in a time zone
/// where the clocks go back meanwhile, last an hour more.
const CLOCK_CHANGE: Duration = Duration::hours(1);

const NOT_ONESELF: &str = "Désignez un collègue : vous ne pouvez pas vous remplacer vous-même.";
const UNKNOWN_DELEGATE: &str = "Ce collègue n’existe pas ou son compte n’est pas actif.";
const ENDS_IN_THE_PAST: &str = "La fin de l’absence doit être à venir.";
const ENDS_BEFORE_IT_STARTS: &str = "La fin de l’absence doit suivre son début.";
const TOO_LONG: &str = "Une absence déclarée dure au plus 90 jours : déclarez la suite plus tard.";
const NONE_DECLARED: &str = "Aucune absence déclarée";

/// An absence to declare: who stands in, from (now when absent) until when.
#[derive(Debug, Clone, Copy)]
pub struct AbsenceInput {
    pub delegate_user_id: uuid::Uuid,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: DateTime<Utc>,
}

/// The signed-in person's delegation, if it has not ended, those they
/// stand in for, or will, and whom they may name.
pub struct Delegations {
    pub own: Option<ApprovalDelegation>,
    pub standing_in_for: Vec<ApprovalDelegation>,
    pub candidates: Vec<Colleague>,
}

/// The signed-in person's delegations; the colleagues they may name only
/// if they `decide` requests themselves (hold `approval:decide`): nobody
/// else learns who in the company may validate.
pub async fn delegations(
    db: &Database,
    identity: AuthIdentity,
    decide: bool,
    now: DateTime<Utc>,
) -> anyhow::Result<Delegations> {
    let mut tx = db.begin(identity.tenant_id).await?;
    let own = get_delegation(&mut tx, identity.user_id, now).await?;
    let standing_in_for = delegations_to(&mut tx, identity.user_id, now).await?;
    let candidates = if decide {
        possible_stand_ins(&mut tx, identity.user_id, now).await?
    } else {
        Vec::new()
    };
    tx.commit().await?;
    Ok(Delegations {
        own,
        standing_in_for,
        candidates,
    })
}

/// The period `input` declares, from its start (`now` when absent) to its
/// end: ending in the future, after it starts, within 90 days.
pub fn absence_period(
    input: &AbsenceInput,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), DomainError> {
    let starts_at = input.starts_at.unwrap_or(now);
    let ends_at = input.ends_at;
    if ends_at <= now {
        return Err(DomainError::validation(ENDS_IN_THE_PAST));
    }
    if ends_at <= starts_at {
        return Err(DomainError::validation(ENDS_BEFORE_IT_STARTS));
    }
    if ends_at - starts_at > MAX_ABSENCE + CLOCK_CHANGE {
        return Err(DomainError::validation(TOO_LONG));
    }
    Ok((starts_at, ends_at))
}

/// The delegate named: an active account of the company, read in it (another
/// company's account is unknown here, like an absent or inactive one).
pub async fn named_delegate(
    conn: &mut PgConnection,
    delegate_id: uuid::Uuid,
) -> anyhow::Result<DirectoryEntry> {
    match get_user(conn, delegate_id).await? {
        Some(delegate) if delegate.status == "active" => Ok(delegate),
        _ => Err(DomainError::unknown(UNKNOWN_DELEGATE, "user", delegate_id).into()),
    }
}

/// Declares, or replaces, the signed-in person's absence.
pub async fn declare_absence(
    db: &Database,
    identity: AuthIdentity,
    input: AbsenceInput,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    if input.delegate_user_id == identity.user_id {
        return Err(DomainError::validation(NOT_ONESELF).into());
    }
    let (starts_at, ends_at) = absence_period(&input, now)?;
    let mut tx = db.begin(identity.tenant_id).await?;
    named_delegate(&mut tx, input.delegate_user_id).await?;
    let delegation = NewDelegation {
        tenant_id: identity.tenant_id,
        user_id: identity.user_id,
        delegate_user_id: input.delegate_user_id,
        starts_at,
        ends_at,
        // Declared by the person: no longer an administrator's.
        declared_by: None,
    };
    set_delegation(&mut tx, delegation).await?;
    let entry = AuditEntry::new(
        identity.tenant_id,
        "user",
        "approval.delegation_set",
        "success",
    )
    .by_user(identity.user_id)
    .resource("user", identity.user_id)
    .metadata(json!({
        "delegateId": input.delegate_user_id,
        "startsAt": starts_at,
        "endsAt": ends_at,
    }));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

/// Withdraws the signed-in person's absence before its end.
pub async fn end_absence(
    db: &Database,
    identity: AuthIdentity,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let mut tx = db.begin(identity.tenant_id).await?;
    if !delete_delegation(&mut tx, identity.user_id, now).await? {
        return Err(DomainError::not_found(NONE_DECLARED).into());
    }
    let entry = AuditEntry::new(
        identity.tenant_id,
        "user",
        "approval.delegation_ended",
        "success",
    )
    .by_user(identity.user_id)
    .resource("user", identity.user_id);
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}
