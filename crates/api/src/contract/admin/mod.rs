//! What the administration screens are answered with, and what they write.

mod activity;
mod approval_routing;
mod assignments;
mod connectors;
mod llm;
mod organization;
mod policies;
mod roles;
mod skills;
mod users;

pub use activity::{
    AuditActor, AuditEntry, AuditLog, AuditOutcome, UsageKind, UsageRow, UsageSummary,
};
pub use approval_routing::{ApprovalRoutingUpdate, ApprovalRoutingView};
pub use assignments::{Assignment, AssignmentScope, RoleGrant};
pub use connectors::{
    ConnectorStatus, ConnectorTest, ConnectorTrial, ConnectorType, ConnectorUpdate, ConnectorView,
    Connectors, DelegablePermission, McpEndpoint, NewConnector, TestedTool, ToolPolicy,
    ToolPolicyChange, risks,
};
pub use llm::{LlmChoice, LlmSettingsView, LlmTestResult, LlmView, PlatformModel, ProviderKind};
pub use policies::{
    ConditionAttribute, NewPolicy, Policies, PolicyEffect, PolicyUpdate, PolicyView,
    RestrictablePermission,
};

pub use organization::{
    NewOrgUnit, OrgPerson, OrgUnitType, OrgUnitUpdate, OrgUnitView, Organization,
};
pub use roles::{Enforcement, NewRole, PermissionView, RoleUpdate, RoleView, Roles};
pub use skills::{
    NewSkill, NewSkillVersion, SkillActivation, SkillDetail, SkillList, SkillStatus, SkillUpdate,
    SkillVersionCreated, SkillVersionStatus, SkillVersionView, SkillView,
};
pub use users::{
    AccountStatus, AccountUpdate, Directory, DirectoryRights, DirectoryUser, InvitationIssued,
    IssuedInvitation, NewAccount, ResetLinkSent, Sensitivity, StatusChange, TemporaryPassword,
    UserCreated,
};
