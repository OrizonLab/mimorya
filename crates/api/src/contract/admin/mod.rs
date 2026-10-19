//! What the administration screens are answered with, and what they write.

mod activity;
mod assignments;
mod connectors;
mod llm;
mod organization;
mod policies;
mod roles;
mod users;

pub use activity::{
    AuditActor, AuditEntry, AuditLog, AuditOutcome, UsageKind, UsageRow, UsageSummary,
};
pub use assignments::{Assignment, AssignmentScope, RoleGrant};
pub use connectors::{
    ConnectorStatus, ConnectorTest, ConnectorType, ConnectorUpdate, ConnectorView, Connectors,
    DelegablePermission, McpEndpoint, NewConnector, TestedTool, ToolPolicy, ToolPolicyChange,
    risks,
};
pub use llm::{LlmChoice, LlmSettingsView, LlmTestResult, LlmView, PlatformModel, ProviderKind};
pub use policies::{
    ConditionAttribute, NewPolicy, Policies, PolicyEffect, PolicyUpdate, PolicyView,
    RestrictablePermission,
};

pub use organization::{
    NewOrgUnit, OrgPerson, OrgUnitType, OrgUnitUpdate, OrgUnitView, Organization,
};
pub use roles::{
    Enforcement, NewRole, PermissionView, RoleUpdate, RoleView, Roles, SkillList, SkillStatus,
    SkillView,
};
pub use users::{
    AccountStatus, AccountUpdate, Directory, DirectoryRights, DirectoryUser, InvitationIssued,
    IssuedInvitation, NewAccount, ResetLinkSent, Sensitivity, StatusChange, TemporaryPassword,
    UserCreated,
};
