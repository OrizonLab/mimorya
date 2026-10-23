//! Absences declared by an account administrator (ADR-078) for a
//! collaborator who cannot declare their own (illness, accident): the same
//! absence as theirs (ADR-071), naming whom stands in for them on the
//! requests they decide as a manager, for at most 90 days, granting no
//! right. The collaborator sees who declared it, is told by e-mail when a
//! relay is configured, and keeps the hand: they end it, or declare their
//! own. Company-wide `user:manage`, never on one's own account (one's own
//! absence is declared in « Validations »), under the administration lock,
//! audited.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::repositories::approval_delegations::{
    AbsentPerson, ApprovalDelegation, Colleague, NewDelegation, absent_person, delete_delegation,
    get_delegation, possible_stand_ins, set_delegation,
};
use mimorya_db::repositories::audit::append_audit;
use serde_json::json;
use uuid::Uuid;

use super::absence_mail::{AbsenceMail, absence_mail};
use super::{LinkSender, audit_entry, begin_change, refuse_self};
use crate::services::AccessContext;
use crate::services::runs::{AbsenceInput, absence_period, named_delegate};

const PERMISSION: &str = "user:manage";

const UNKNOWN_ACCOUNT: &str = "Compte introuvable";
const NOT_THE_ABSENT: &str = "Désignez comme remplaçant un autre collaborateur que l’absent.";
const INACTIVE: &str = "Ce compte n’est pas actif : une absence serait sans effet.";
const NONE_DECLARED: &str = "Aucune absence déclarée pour ce collaborateur";

/// A collaborator's absence, as the administrator reads it, and whom they
/// may name.
pub struct AbsenceAdmin {
    pub absence: Option<ApprovalDelegation>,
    pub candidates: Vec<Colleague>,
}

/// The absence of `user_id`, unless it ended, and the colleagues who may
/// stand in for them: those holding `approval:decide` on a unit or the
/// company (ADR-073).
pub async fn absence_of(
    sender: &LinkSender<'_>,
    access: &AccessContext,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> anyhow::Result<AbsenceAdmin> {
    access.require_tenant_wide(PERMISSION)?;
    refuse_self(access, user_id)?;
    let mut tx = sender.db.begin(access.subject.tenant_id).await?;
    if absent_person(&mut tx, user_id).await?.is_none() {
        return Err(DomainError::not_found(UNKNOWN_ACCOUNT).into());
    }
    let absence = get_delegation(&mut tx, user_id, now).await?;
    let candidates = possible_stand_ins(&mut tx, user_id, now).await?;
    tx.commit().await?;
    Ok(AbsenceAdmin {
        absence,
        candidates,
    })
}

/// Declares, or replaces, the absence of `user_id`; whether they were told
/// by e-mail.
pub async fn declare_absence_for(
    sender: &LinkSender<'_>,
    access: &AccessContext,
    user_id: Uuid,
    input: AbsenceInput,
    now: DateTime<Utc>,
) -> anyhow::Result<bool> {
    access.require_tenant_wide(PERMISSION)?;
    refuse_self(access, user_id)?;
    if input.delegate_user_id == user_id {
        return Err(DomainError::validation(NOT_THE_ABSENT).into());
    }
    let (starts_at, ends_at) = absence_period(&input, now)?;
    let mut tx = begin_change(sender.db, access, PERMISSION).await?;
    let Some(absent) = absent_person(&mut tx, user_id).await? else {
        return Err(DomainError::not_found(UNKNOWN_ACCOUNT).into());
    };
    if absent.status != "active" {
        return Err(DomainError::conflict(INACTIVE).into());
    }
    let delegate = named_delegate(&mut tx, input.delegate_user_id).await?;
    let delegation = NewDelegation {
        tenant_id: access.subject.tenant_id,
        user_id,
        delegate_user_id: input.delegate_user_id,
        starts_at,
        ends_at,
        declared_by: Some(access.subject.user_id),
    };
    set_delegation(&mut tx, delegation).await?;
    let metadata = json!({
        "delegateId": input.delegate_user_id,
        "startsAt": starts_at,
        "endsAt": ends_at,
    });
    let entry = audit_entry(access, "approval.delegation_set", "user", user_id);
    append_audit(&mut tx, entry.metadata(metadata)).await?;
    tx.commit().await?;
    let period = Some((starts_at, ends_at));
    Ok(tell(
        sender,
        access,
        user_id,
        &absent,
        &delegate.display_name,
        period,
    )
    .await)
}

/// Ends the absence of `user_id` before its end; whether they were told by
/// e-mail.
pub async fn end_absence_for(
    sender: &LinkSender<'_>,
    access: &AccessContext,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> anyhow::Result<bool> {
    access.require_tenant_wide(PERMISSION)?;
    refuse_self(access, user_id)?;
    let mut tx = begin_change(sender.db, access, PERMISSION).await?;
    let Some(absent) = absent_person(&mut tx, user_id).await? else {
        return Err(DomainError::not_found(UNKNOWN_ACCOUNT).into());
    };
    let current = get_delegation(&mut tx, user_id, now).await?;
    if !delete_delegation(&mut tx, user_id, now).await? {
        return Err(DomainError::not_found(NONE_DECLARED).into());
    }
    let entry = audit_entry(access, "approval.delegation_ended", "user", user_id);
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    let delegate = current.map(|d| d.delegate_name).unwrap_or_default();
    Ok(tell(sender, access, user_id, &absent, &delegate, None).await)
}

/// Tells the collaborator what the administrator did, when a relay is
/// configured and their account active. A failure is logged without the
/// recipient (the relay's answer may name them): the absence stands.
async fn tell(
    sender: &LinkSender<'_>,
    access: &AccessContext,
    user_id: Uuid,
    absent: &AbsentPerson,
    delegate: &str,
    period: Option<(DateTime<Utc>, DateTime<Utc>)>,
) -> bool {
    let Some(mailer) = sender.mailer else {
        return false;
    };
    if absent.status != "active" {
        return false;
    }
    let link = format!("{}/approvals", sender.web_origin);
    let mail = absence_mail(&AbsenceMail {
        product: sender.product,
        company: &absent.company,
        name: &absent.display_name,
        email: &absent.email,
        administrator: &access.profile.display_name,
        delegate,
        period,
        link: &link,
    });
    match mailer.send(&mail).await {
        Ok(()) => true,
        Err(_) => {
            tracing::warn!(user = %user_id, "Absence e-mail not sent");
            false
        }
    }
}
