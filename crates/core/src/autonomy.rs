//! Autonomy levels decide what the agent may do with a tool once RBAC/ABAC
//! has allowed it. Authorization answers "may this user do it?"; autonomy
//! answers "may the agent do it on its own, or must a human validate first?".

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Risk class declared by every tool, ordered from harmless to irreversible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    /// Retrieval without side effects.
    Read,
    /// Creates internal drafts/proposals, no effect outside the platform.
    Draft,
    /// Modifies data in a connected system (reversible).
    Write,
    /// Sends something outside the company (email, client message…).
    External,
    /// Deletion, payment, anything that cannot be undone.
    Irreversible,
}

impl ToolRisk {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Draft => "draft",
            Self::Write => "write",
            Self::External => "external",
            Self::Irreversible => "irreversible",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::Read,
            Self::Draft,
            Self::Write,
            Self::External,
            Self::Irreversible,
        ]
        .into_iter()
        .find(|r| r.as_str() == value)
    }

    fn has_side_effect(self) -> bool {
        !matches!(self, Self::Read | Self::Draft)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    ReadOnly,
    Propose,
    ExecuteWithApproval,
    AutonomousLimited,
}

impl AutonomyLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Propose => "propose",
            Self::ExecuteWithApproval => "execute_with_approval",
            Self::AutonomousLimited => "autonomous_limited",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::ReadOnly,
            Self::Propose,
            Self::ExecuteWithApproval,
            Self::AutonomousLimited,
        ]
        .into_iter()
        .find(|l| l.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyPolicy {
    pub level: AutonomyLevel,
    /// Tools explicitly allowed to run without approval at `autonomous_limited`.
    pub autonomous_tools: HashSet<String>,
}

impl AutonomyPolicy {
    pub fn new(level: AutonomyLevel) -> Self {
        Self {
            level,
            autonomous_tools: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionDecision {
    Allow,
    RequireApproval { reason: String },
    Deny { reason: String },
}

impl ExecutionDecision {
    pub fn is_deny(&self) -> bool {
        matches!(self, Self::Deny { .. })
    }
}

/// What the autonomy policy needs to know about a tool.
#[derive(Debug, Clone, Copy)]
pub struct ToolRiskProfile<'a> {
    pub name: &'a str,
    pub risk: ToolRisk,
    /// Forces human validation whatever the autonomy level.
    pub requires_approval: bool,
}

pub fn decide_execution(policy: &AutonomyPolicy, tool: ToolRiskProfile<'_>) -> ExecutionDecision {
    let side_effect = tool.risk.has_side_effect();
    let deny = |reason: &str| ExecutionDecision::Deny {
        reason: reason.into(),
    };
    let approval = |reason: &str| ExecutionDecision::RequireApproval {
        reason: reason.into(),
    };

    if policy.level == AutonomyLevel::ReadOnly && tool.risk != ToolRisk::Read {
        return deny("Niveau d’autonomie « lecture seule »");
    }
    if policy.level == AutonomyLevel::Propose && side_effect {
        return deny(
            "Niveau d’autonomie « proposition » : l’action doit être proposée, pas exécutée",
        );
    }
    if tool.requires_approval {
        return approval("Cet outil exige une validation humaine");
    }
    if !side_effect {
        return ExecutionDecision::Allow;
    }
    if tool.risk == ToolRisk::Irreversible {
        return approval("Action irréversible");
    }
    if policy.level == AutonomyLevel::AutonomousLimited
        && policy.autonomous_tools.contains(tool.name)
    {
        return ExecutionDecision::Allow;
    }
    approval(if tool.risk == ToolRisk::External {
        "Action à impact externe"
    } else {
        "Action modifiant des données"
    })
}
