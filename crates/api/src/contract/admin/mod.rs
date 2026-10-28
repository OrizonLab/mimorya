//! What the administration screens are answered with, and what they write.

mod absences;
mod activity;
mod approval_routing;
mod assignments;
mod connectors;
mod domain_proofs;
mod embedding;
mod gateways;
mod google_workspace;
mod identity_providers;
mod llm;
mod memory_settings;
mod microsoft365;
mod mission_settings;
mod organization;
mod policies;
mod rest_connectors;
mod rest_import;
mod roles;
mod skill_eval_cases;
mod skill_evaluations;
mod skills;
mod users;

pub use absences::{AbsenceRecorded, CollaboratorAbsence};
pub use activity::{
    AuditActor, AuditEntry, AuditLog, AuditOutcome, UsageKind, UsageRow, UsageSummary,
};
pub use approval_routing::{ApprovalRoutingUpdate, ApprovalRoutingView};
pub use assignments::{Assignment, AssignmentScope, RoleGrant};
pub use connectors::{
    ConnectorDetails, ConnectorStatus, ConnectorTest, ConnectorTrial, ConnectorType,
    ConnectorUpdate, ConnectorView, Connectors, DelegablePermission, McpEndpoint, NewConnector,
    TestedTool, ToolPolicy, ToolPolicyChange, risks,
};
pub use domain_proofs::{DomainProofRequest, DomainProofView};
pub use embedding::{
    EmbeddingChoice, EmbeddingKind, EmbeddingSettingsView, EmbeddingTestResult, EmbeddingView,
};
pub use gateways::{GatewayUpdate, GatewayView, Gateways, NewSlackGateway};
pub use google_workspace::{GoogleWorkspaceApp, NewGoogleWorkspaceConnector};
pub use identity_providers::{
    IdentityProviderUpdate, IdentityProviderView, IdentityProvidersView, NewIdentityProvider,
};
pub use llm::{LlmChoice, LlmSettingsView, LlmTestResult, LlmView, PlatformModel, ProviderKind};
pub use memory_settings::{MemorySettingsUpdate, MemorySettingsView};
pub use microsoft365::{Microsoft365App, NewMicrosoft365Connector};
pub use mission_settings::{MissionSettingsUpdate, MissionSettingsView};
pub use policies::{
    ConditionAttribute, NewPolicy, Policies, PolicyEffect, PolicyUpdate, PolicyView,
    RestrictablePermission,
};
pub use rest_connectors::{
    HttpMethod, NewRestConnector, ParameterLocation, ParameterType, RestApi, RestOperation,
    RestParameter,
};
pub use rest_import::{OpenApiDescription, RestImport};

pub use organization::{
    NewOrgUnit, OrgPerson, OrgUnitType, OrgUnitUpdate, OrgUnitView, Organization,
};
pub use roles::{Enforcement, NewRole, PermissionView, RoleUpdate, RoleView, Roles};
pub use skill_eval_cases::{SkillEvalCaseInput, SkillEvalCaseView};
pub use skill_evaluations::{
    CaseOutcomeView, CaseResultView, EvaluatedCaseView, EvaluationRequest, EvaluationStatus,
    SkillEvaluationDetail, SkillEvaluationView, SkillEvaluations, ToolCallView,
};
pub use skills::{
    NewSkill, NewSkillVersion, SkillActivation, SkillDetail, SkillList, SkillRejection,
    SkillStatus, SkillUpdate, SkillVersionCreated, SkillVersionStatus, SkillVersionView, SkillView,
};
pub use users::{
    AccountStatus, AccountUpdate, Directory, DirectoryRights, DirectoryUser, InvitationIssued,
    IssuedInvitation, NewAccount, ResetLinkSent, Sensitivity, StatusChange, TemporaryPassword,
    UserCreated,
};
