//! The company's rules of its router (ADR-207, step 2): for each kind of
//! run, the model it starts with. Read only while the router is on; a skill
//! loaded later may still switch the run to the escalation model.

/// The kinds of run the rules tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    /// A person's turn of conversation, whatever its channel (the web, a
    /// Slack, Telegram or Teams gateway).
    Conversation,
    /// A stage of a mission (ADR-089).
    Mission,
    /// A run a scheduled task started (ADR-080).
    ScheduledTask,
    /// A run another system started through a hook (ADR-109).
    Hook,
}

/// The model a run of a kind starts with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EscalationRoute {
    #[default]
    Main,
    Escalation,
    /// The one a short classification of the request by the main model
    /// names; the main one should it name none.
    Classification,
}

impl EscalationRoute {
    pub const ALL: [EscalationRoute; 3] = [Self::Main, Self::Escalation, Self::Classification];

    /// As stored (`tenant_model_escalation.route_*`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Escalation => "escalation",
            Self::Classification => "classification",
        }
    }

    pub fn parse(stored: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|route| route.as_str() == stored)
    }
}

/// The rule of each kind of run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EscalationRoutes {
    pub conversation: EscalationRoute,
    pub mission: EscalationRoute,
    pub scheduled_task: EscalationRoute,
    pub hook: EscalationRoute,
}

impl EscalationRoutes {
    /// The model a run of `kind` starts with.
    pub fn of(&self, kind: RunKind) -> EscalationRoute {
        match kind {
            RunKind::Conversation => self.conversation,
            RunKind::Mission => self.mission,
            RunKind::ScheduledTask => self.scheduled_task,
            RunKind::Hook => self.hook,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_run_has_its_rule_and_each_rule_reads_back_as_stored() {
        let routes = EscalationRoutes {
            conversation: EscalationRoute::Main,
            mission: EscalationRoute::Escalation,
            scheduled_task: EscalationRoute::Classification,
            hook: EscalationRoute::Escalation,
        };
        assert_eq!(routes.of(RunKind::Conversation), EscalationRoute::Main);
        assert_eq!(routes.of(RunKind::Mission), EscalationRoute::Escalation);
        assert_eq!(
            routes.of(RunKind::ScheduledTask),
            EscalationRoute::Classification
        );
        assert_eq!(routes.of(RunKind::Hook), EscalationRoute::Escalation);
        for route in EscalationRoute::ALL {
            assert_eq!(EscalationRoute::parse(route.as_str()), Some(route));
        }
        assert_eq!(EscalationRoute::parse("Main"), None);
        assert_eq!(
            EscalationRoutes::default().of(RunKind::Hook),
            EscalationRoute::Main
        );
    }
}
