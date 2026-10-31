//! Who pressed a validation's button in a messaging service (ADR-129,
//! ADR-130, ADR-136, ADR-137): the requests they see (made for them, or awaiting
//! them) and those they decide (their own, a manager's they are, or stand
//! in for). Nothing is told of any other. A press is decided alike in each
//! service (`decide_as`): only a request awaiting its presser, that may be
//! validated outside the application, as from the application's screen,
//! audited with the service; otherwise what to tell them.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_core::gateways::{GatewayKind, ShownAction, validated_in_gateway};
use mimorya_db::repositories::approval_delegations::standing_in_for;
use mimorya_db::repositories::approvals::{ApprovalRow, get_approval};
use uuid::Uuid;

use crate::app::Services;
use crate::services::access::AuthIdentity;
use crate::services::runs::{APPROVAL_NOT_FOUND, DecisionInput, stands_in};

/// What a press that decided nothing is told: the decision failed.
pub(super) const NOT_DECIDED: &str = "La validation n’a pas pu être enregistrée : réessayez \
     dans un instant, ou donnez-la dans l’application.";
/// The action is validated in the application only.
const IN_APPLICATION: &str = "Cette action se valide dans l’application, qui en \
     montre les paramètres exacts : rien n’a été décidé.";
/// The action awaits a manager's decision.
const AWAITS_A_MANAGER: &str = "Cette action attend la validation de votre \
     responsable : rien n’a été décidé.";

/// An action awaiting validation, as a message of the bot shows it.
pub(super) fn shown_action(approval: &ApprovalRow) -> ShownAction<'_> {
    ShownAction {
        approval_id: approval.id,
        summary: &approval.action_summary,
        risk: &approval.risk,
        parameters: &approval.payload,
    }
}

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

    /// The requests `ids` a message of the bot shows, as they are now, in
    /// order; none when they do not see one of them (an account linked
    /// since to someone else learns nothing of the actions of the person
    /// before), nor when one is no more.
    pub(super) async fn shown_now(
        &self,
        services: &Services,
        tenant_id: Uuid,
        ids: &[Uuid],
    ) -> anyhow::Result<Option<Vec<ApprovalRow>>> {
        let mut approvals = Vec::with_capacity(ids.len());
        let mut tx = services.db.begin(tenant_id).await?;
        for id in ids {
            match get_approval(&mut tx, *id).await? {
                Some(approval) if self.sees(&approval) => approvals.push(approval),
                _ => return Ok(None),
            }
        }
        tx.commit().await?;
        Ok(Some(approvals))
    }
}

/// A request decided from a messaging service.
pub(super) struct Decided {
    /// Whether its run resumed.
    pub(super) resumed: bool,
    /// Whether it was made for whom decided it.
    pub(super) own: bool,
}

/// Decides, as `identity`, the request `approval_id` (`approved` or not)
/// pressed in the service `kind`: who pressed it, and what became of it,
/// or what to tell them when nothing was decided.
pub(super) async fn decide_as(
    services: &Services,
    identity: AuthIdentity,
    (approval_id, approved): (Uuid, bool),
    kind: GatewayKind,
) -> anyhow::Result<(Concerned, Result<Decided, String>)> {
    let mut tx = services.db.begin(identity.tenant_id).await?;
    let approval = get_approval(&mut tx, approval_id).await?;
    let concerned = Concerned {
        user_id: identity.user_id,
        standing_in_for: standing_in_for(&mut tx, identity.user_id, Utc::now()).await?,
    };
    tx.commit().await?;
    // Their own action, or one awaiting them: nothing told of any other.
    let Some(approval) = approval.filter(|a| concerned.sees(a)) else {
        return Ok((concerned, Err(APPROVAL_NOT_FOUND.to_owned())));
    };
    if !concerned.decides(&approval) {
        return Ok((concerned, Err(AWAITS_A_MANAGER.to_owned())));
    }
    if !validated_in_gateway(&approval.risk, &approval.payload) {
        return Ok((concerned, Err(IN_APPLICATION.to_owned())));
    }
    let input = DecisionInput {
        approval_id,
        approved,
        comment: None,
        gateway: Some(kind.as_str()),
    };
    let decided = match services.runs.decide(identity, input).await {
        Ok(decided) => Ok(Decided {
            resumed: decided.resumed,
            own: approval.requested_for_user_id == identity.user_id,
        }),
        Err(error) => match error.downcast_ref::<DomainError>() {
            Some(refusal) => Err(refusal.message.clone()),
            None => return Err(error),
        },
    };
    Ok((concerned, decided))
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
