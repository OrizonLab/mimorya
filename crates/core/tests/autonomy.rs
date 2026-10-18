//! Autonomy matrix: what the agent may run alone, with approval, or never.

use mimorya_core::autonomy::{
    AutonomyLevel, AutonomyPolicy, ExecutionDecision, ToolRisk, ToolRiskProfile, decide_execution,
};

fn kind(decision: ExecutionDecision) -> &'static str {
    match decision {
        ExecutionDecision::Allow => "allow",
        ExecutionDecision::RequireApproval { .. } => "require_approval",
        ExecutionDecision::Deny { .. } => "deny",
    }
}

fn tool(name: &str, risk: ToolRisk, requires_approval: bool) -> ToolRiskProfile<'_> {
    ToolRiskProfile {
        name,
        risk,
        requires_approval,
    }
}

#[test]
fn follows_the_autonomy_matrix() {
    use AutonomyLevel::*;
    use ToolRisk::*;
    let matrix = [
        (ReadOnly, Read, "allow"),
        (ReadOnly, Draft, "deny"),
        (ReadOnly, Write, "deny"),
        (Propose, Draft, "allow"),
        (Propose, Write, "deny"),
        (Propose, External, "deny"),
        (ExecuteWithApproval, Read, "allow"),
        (ExecuteWithApproval, Draft, "allow"),
        (ExecuteWithApproval, Write, "require_approval"),
        (ExecuteWithApproval, External, "require_approval"),
        (ExecuteWithApproval, Irreversible, "require_approval"),
        (AutonomousLimited, Write, "require_approval"),
        (AutonomousLimited, Irreversible, "require_approval"),
    ];
    for (level, risk, expected) in matrix {
        let decision = decide_execution(&AutonomyPolicy::new(level), tool("tool_x", risk, false));
        assert_eq!(kind(decision), expected, "{level:?} × {risk:?}");
    }
}

#[test]
fn runs_allow_listed_tools_autonomously_except_irreversible_ones() {
    let mut policy = AutonomyPolicy::new(AutonomyLevel::AutonomousLimited);
    policy
        .autonomous_tools
        .extend(["tool_write".to_string(), "tool_irreversible".to_string()]);
    assert_eq!(
        kind(decide_execution(
            &policy,
            tool("tool_write", ToolRisk::Write, false)
        )),
        "allow"
    );
    assert_eq!(
        kind(decide_execution(
            &policy,
            tool("tool_irreversible", ToolRisk::Irreversible, false)
        )),
        "require_approval"
    );
}

#[test]
fn honours_tools_that_always_require_approval() {
    let policy = AutonomyPolicy::new(AutonomyLevel::AutonomousLimited);
    assert_eq!(
        kind(decide_execution(
            &policy,
            tool("tool_draft", ToolRisk::Draft, true)
        )),
        "require_approval"
    );
}
