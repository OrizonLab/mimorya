//! Who pressed a validation's button in a messaging service (ADR-129,
//! ADR-130, ADR-136): the requests they see (made for them, or awaiting
//! them) and those they decide (their own, a manager's they are, or stand
//! in for). Nothing is told of any other. What a press that decided
//! nothing is told, alike in each service.

use mimorya_db::repositories::approvals::ApprovalRow;
use uuid::Uuid;

use crate::services::runs::stands_in;

/// What a press that decided nothing is told: the decision failed.
pub(super) const NOT_DECIDED: &str = "La validation n’a pas pu être enregistrée : réessayez \
     dans un instant, ou donnez-la dans l’application.";
/// The action is validated in the application only.
pub(super) const IN_APPLICATION: &str = "Cette action se valide dans l’application, qui en \
     montre les paramètres exacts : rien n’a été décidé.";
/// The action awaits a manager's decision.
pub(super) const AWAITS_A_MANAGER: &str = "Cette action attend la validation de votre \
     responsable : rien n’a été décidé.";

/// Who pressed a button: the requests they see, and those they decide.
pub(super) struct Concerned {
    pub(super) user_id: Uuid,
    /// Those whose absence they cover now (ADR-071).
    pub(super) standing_in_for: Vec<Uuid>,
}

impl Concerned {
    /// Whether `approval` is made for them, or awaits them.
    pub(super) fn sees(&self, approval: &ApprovalRow) -> bool {
        approval.requested_for_user_id == self.user_id || self.decides(approval)
    }

    /// Whether `approval` awaits their decision: their own, or a manager's
    /// they are, or stand in for.
    pub(super) fn decides(&self, approval: &ApprovalRow) -> bool {
        approval.decider() == Some(self.user_id) || stands_in(approval, &self.standing_in_for)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn pending() -> ApprovalRow {
        ApprovalRow {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            requested_for_user_id: Uuid::new_v4(),
            tool_name: "planning_apply".into(),
            tool_call_id: "call-1".into(),
            action_summary: "Appliquer".into(),
            payload: json!({}),
            risk: "write".into(),
            reason: String::new(),
            status: "pending".into(),
            decided_by_user_id: None,
            decided_at: None,
            decision_comment: None,
            expires_at: None,
            created_at: Utc::now(),
            routing: "requester".into(),
            approver_user_id: None,
        }
    }

    #[test]
    fn whom_a_request_awaits_decides_it() {
        let (requester, manager, stand_in) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let mut own = pending();
        own.requested_for_user_id = requester;
        let mut managed = own.clone();
        managed.routing = "manager".into();
        managed.approver_user_id = Some(manager);
        let person = |user_id, standing_in_for| Concerned {
            user_id,
            standing_in_for,
        };
        let theirs = person(requester, vec![]);
        assert!(theirs.sees(&own) && theirs.decides(&own));
        // A manager's: seen by whom the agent acts for, not decided.
        assert!(theirs.sees(&managed) && !theirs.decides(&managed));
        let boss = person(manager, vec![]);
        assert!(boss.decides(&managed) && !boss.sees(&own));
        let standing = person(stand_in, vec![manager]);
        assert!(standing.decides(&managed) && !standing.sees(&own));
        assert!(!person(stand_in, vec![]).sees(&managed));
    }
}
