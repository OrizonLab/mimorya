//! Who validates an action for someone (ADR-059). A company names the risks
//! its managers validate; any other action is validated by the person the
//! agent acts for. The manager is found up the organisation, but a position
//! grants nothing by itself: they must hold `approval:decide` over the
//! person, on an active account, when the request is made and again when
//! they decide.

use uuid::Uuid;

use crate::authz::{OrgTree, ResourceRef};

pub const DECIDE: &str = "approval:decide";

/// What `approval:decide` is checked on: a request made for `requester`,
/// in their primary unit.
pub fn approval_resource(
    tenant_id: Uuid,
    requester: Uuid,
    primary_unit: Option<Uuid>,
) -> ResourceRef {
    ResourceRef::new("approval", tenant_id)
        .owner(Some(requester))
        .org_unit(primary_unit)
}

/// Who may validate for `requester`, nearest first: the lead of their
/// primary unit, then of each unit above, never the requester. Each still
/// has to hold `approval:decide` over them.
pub fn candidate_managers(
    tree: &OrgTree,
    requester: Uuid,
    primary_unit: Option<Uuid>,
) -> Vec<Uuid> {
    let Some(unit) = primary_unit.and_then(|id| tree.get(id)) else {
        return Vec::new();
    };
    let mut leads: Vec<Uuid> = Vec::new();
    for lead in std::iter::once(unit)
        .chain(tree.ancestors(unit.id))
        .filter_map(|u| u.lead_user_id)
    {
        if lead != requester && !leads.contains(&lead) {
            leads.push(lead);
        }
    }
    leads
}

/// The manager who validates for `requester`: the nearest candidate for
/// whom `may_decide` holds. `None` when nobody does.
pub fn manager_for(
    tree: &OrgTree,
    requester: Uuid,
    primary_unit: Option<Uuid>,
    may_decide: impl FnMut(&Uuid) -> bool,
) -> Option<Uuid> {
    candidate_managers(tree, requester, primary_unit)
        .into_iter()
        .find(may_decide)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authz::Actor;
    use crate::demo::{self, tenant, unit, user};

    /// Whether `lead` holds `approval:decide` over `requester`, as the demo
    /// company grants it.
    fn holds(requester: Uuid, primary: Uuid) -> impl FnMut(&Uuid) -> bool {
        let engine = demo::authorization_engine();
        move |&lead| {
            let resource = approval_resource(tenant::ID, requester, Some(primary));
            engine
                .check(&demo::subject(lead), DECIDE, &resource, &Actor::user())
                .allowed
        }
    }

    #[test]
    fn the_nearest_lead_holding_the_permission_validates() {
        let tree = demo::org_tree();
        let route = |requester, primary| {
            manager_for(&tree, requester, Some(primary), holds(requester, primary))
        };
        // A technician: their team lead.
        assert_eq!(route(user::LUCAS, unit::TEAM_NORTH), Some(user::JULIEN));
        assert_eq!(route(user::HUGO, unit::TEAM_SOUTH), Some(user::NADIA));
        // A team lead leads their own unit: the lead above.
        assert_eq!(route(user::JULIEN, unit::TEAM_NORTH), Some(user::MARC));
        // The leads above Marc (the director) do not hold the permission.
        assert_eq!(route(user::MARC, unit::TECHNICAL), None);
        assert_eq!(route(user::PAUL, unit::ADMIN), None);
    }

    #[test]
    fn a_position_without_the_permission_is_passed_over() {
        let tree = demo::org_tree();
        // Julien leads Lucas's team but may no longer decide: Marc above.
        let mut holds = holds(user::LUCAS, unit::TEAM_NORTH);
        let routed = manager_for(&tree, user::LUCAS, Some(unit::TEAM_NORTH), |lead| {
            *lead != user::JULIEN && holds(lead)
        });
        assert_eq!(routed, Some(user::MARC));
        // Without a primary unit, nobody.
        assert_eq!(manager_for(&tree, user::LUCAS, None, |_| true), None);
        // Each lead once, nearest first, never the requester.
        assert_eq!(
            candidate_managers(&tree, user::JULIEN, Some(unit::TEAM_NORTH)),
            [user::MARC, user::CLAIRE]
        );
    }
}
