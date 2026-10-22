//! Whom a notice or a reminder goes to (ADR-060, ADR-062): the manager the
//! requests await or, while that manager is away, whom they named to stand
//! in for them (ADR-071), provided that colleague may decide for whom the
//! requests are made: `approval:decide` over them, as at the decision. A
//! colleague who could not decide them is never told of them.

use chrono::Utc;
use mimorya_db::Database;
use mimorya_db::repositories::approval_delegations::stand_in_for;
use mimorya_db::repositories::approval_notices::ApprovalNotice;
use uuid::Uuid;

use super::super::approval_routing::may_decide_for;
use crate::services::access::AccessService;

/// Who is told, and in whose absence.
pub struct Recipient {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    /// The absent manager's name, when a stand-in is told.
    pub absent: Option<String>,
}

impl Recipient {
    fn manager(request: &ApprovalNotice) -> Self {
        Self {
            user_id: request.approver_user_id,
            email: request.approver_email.clone(),
            name: request.approver_name.clone(),
            absent: None,
        }
    }
}

/// Whom to tell of `request` and the others of its run awaiting the same
/// manager, all made for the same person.
pub async fn recipient(
    db: &Database,
    access: &AccessService,
    tenant_id: Uuid,
    request: &ApprovalNotice,
) -> anyhow::Result<Recipient> {
    let mut tx = db.begin(tenant_id).await?;
    let stand_in = stand_in_for(&mut tx, request.approver_user_id, Utc::now()).await?;
    tx.commit().await?;
    let Some(stand_in) = stand_in else {
        return Ok(Recipient::manager(request));
    };
    let requester = request.requested_for_user_id;
    let (Some(delegate), Some(requester_access)) = (
        access.active_context(tenant_id, stand_in.user_id).await?,
        access.active_context(tenant_id, requester).await?,
    ) else {
        return Ok(Recipient::manager(request));
    };
    let unit = requester_access.profile.primary_unit_id;
    if !may_decide_for(&delegate, requester, unit) {
        return Ok(Recipient::manager(request));
    }
    Ok(Recipient {
        user_id: stand_in.user_id,
        email: stand_in.email,
        name: stand_in.display_name,
        absent: Some(request.approver_name.clone()),
    })
}
