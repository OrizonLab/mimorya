//! Who decides a request for validation (ADR-059). The risks a company has
//! its managers validate go to the nearest lead above the person the agent
//! acts for who holds `approval:decide` over them, on an active account:
//! a position grants nothing by itself. Nobody eligible, the action is
//! refused at once rather than left waiting for nobody. Any other request
//! is decided by the person the agent acts for, as before. While the
//! manager is absent, whom they named stands in for them (ADR-071), under
//! the same condition: holding `approval:decide` over the person.

use mimorya_core::approval_routing::{DECIDE, approval_resource, candidate_managers};
use mimorya_core::authz::Actor;
use mimorya_core::autonomy::ToolRisk;
use mimorya_db::Database;
use mimorya_db::repositories::approval_settings::get_manager_risks;
use mimorya_db::repositories::approvals::ApprovalRow;
use uuid::Uuid;

use crate::services::access::{AccessContext, AccessService};

/// Why an action is refused when nobody may validate it.
pub const NOBODY_VALIDATES: &str = "Votre entreprise fait valider ce type d’action par un \
    responsable, et aucun responsable habilité ne peut valider les vôtres : l’action n’est pas \
    exécutée. Votre administrateur peut désigner un responsable de votre unité et lui donner \
    le droit de valider.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// The person the agent acts for decides.
    Requester,
    /// This manager decides.
    Manager(Uuid),
    /// The company has managers validate it, and none may.
    Nobody,
}

pub async fn route(
    db: &Database,
    access: &AccessService,
    tenant_id: Uuid,
    requester: Uuid,
    risk: ToolRisk,
) -> anyhow::Result<Route> {
    let mut tx = db.begin(tenant_id).await?;
    let manager_risks = get_manager_risks(&mut tx).await?;
    tx.commit().await?;
    if !manager_risks.iter().any(|r| r == risk.as_str()) {
        return Ok(Route::Requester);
    }
    // The run acts for an active account (it stops otherwise, ADR-035).
    let Some(requester_access) = access.active_context(tenant_id, requester).await? else {
        return Ok(Route::Nobody);
    };
    let primary_unit = requester_access.profile.primary_unit_id;
    let tree = requester_access.org_tree();
    for lead in candidate_managers(tree, requester, primary_unit) {
        if let Some(lead_access) = access.active_context(tenant_id, lead).await?
            && may_decide_for(&lead_access, requester, primary_unit)
        {
            return Ok(Route::Manager(lead));
        }
    }
    Ok(Route::Nobody)
}

/// Whether `decider` may decide, as a manager, a request made for
/// `requester` (in `primary_unit`): never their own, and only holding
/// `approval:decide` over the requester now.
pub fn may_decide_for(
    decider: &AccessContext,
    requester: Uuid,
    primary_unit: Option<Uuid>,
) -> bool {
    let subject = &decider.subject;
    let resource = approval_resource(subject.tenant_id, requester, primary_unit);
    subject.user_id != requester
        && decider
            .engine
            .check(subject, DECIDE, &resource, &Actor::user())
            .allowed
}

/// Whether `approval` goes to a manager whom someone standing in for
/// `standing_in_for` (those whose absence they cover now, ADR-071) may
/// replace.
pub fn stands_in(approval: &ApprovalRow, standing_in_for: &[Uuid]) -> bool {
    approval.routed_to_manager()
        && approval
            .approver_user_id
            .is_some_and(|manager| standing_in_for.contains(&manager))
}

/// Whether `viewer`, standing in for `standing_in_for`, may decide (approve)
/// `approval`, made for a person whose primary unit is `requester_unit`:
/// pending and theirs to decide, as named or standing in for the manager
/// named, and they may still use the agent (a request made for them) or
/// hold `approval:decide` over that person (as its manager or stand-in).
pub fn can_decide(
    viewer: &AccessContext,
    standing_in_for: &[Uuid],
    approval: &ApprovalRow,
    requester_unit: Option<Uuid>,
) -> bool {
    let entitled = if approval.routed_to_manager() {
        may_decide_for(viewer, approval.requested_for_user_id, requester_unit)
    } else {
        viewer.has("agent:use")
    };
    let theirs =
        approval.decider() == Some(viewer.subject.user_id) || stands_in(approval, standing_in_for);
    approval.status == "pending" && theirs && entitled
}

/// Whether `viewer` may withdraw `approval`: pending, made for them and
/// decided by a manager, and they may still use the agent (withdrawing
/// resumes it). One made for them that they decide, they refuse.
pub fn can_withdraw(viewer: &AccessContext, approval: &ApprovalRow) -> bool {
    approval.status == "pending"
        && approval.routed_to_manager()
        && approval.requested_for_user_id == viewer.subject.user_id
        && viewer.has("agent:use")
}

/// Whether `viewer`, standing in for `standing_in_for`, is shown
/// `approval`, among those listed for them: made for them, decided, or one
/// they may still decide. A manager who lost the right no longer sees what
/// waits, its exact parameters included, nor a stand-in without it.
pub fn shown_to(
    viewer: &AccessContext,
    standing_in_for: &[Uuid],
    approval: &ApprovalRow,
    requester_unit: Option<Uuid>,
) -> bool {
    approval.requested_for_user_id == viewer.subject.user_id
        || approval.status != "pending"
        || can_decide(viewer, standing_in_for, approval, requester_unit)
}
