//! Who decides a request for validation (ADR-059). The risks a company has
//! its managers validate go to the nearest lead above the person the agent
//! acts for who holds `approval:decide` over them, on an active account:
//! a position grants nothing by itself. Nobody eligible, the action is
//! refused at once rather than left waiting for nobody. Any other request
//! is decided by the person the agent acts for, as before.

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

/// Whether `viewer` may decide (approve) `approval`, made for a person whose
/// primary unit is `requester_unit`: pending and theirs to decide, and they
/// may still use the agent (a request made for them) or hold
/// `approval:decide` over that person (as its manager).
pub fn can_decide(
    viewer: &AccessContext,
    approval: &ApprovalRow,
    requester_unit: Option<Uuid>,
) -> bool {
    let entitled = if approval.routed_to_manager() {
        may_decide_for(viewer, approval.requested_for_user_id, requester_unit)
    } else {
        viewer.has("agent:use")
    };
    approval.status == "pending" && approval.decider() == Some(viewer.subject.user_id) && entitled
}

/// Whether `viewer` is shown `approval`: made for them, decided, or one they
/// may still decide. A manager who lost the right no longer sees what
/// waits, its exact parameters included.
pub fn shown_to(
    viewer: &AccessContext,
    approval: &ApprovalRow,
    requester_unit: Option<Uuid>,
) -> bool {
    approval.requested_for_user_id == viewer.subject.user_id
        || approval.status != "pending"
        || can_decide(viewer, approval, requester_unit)
}
