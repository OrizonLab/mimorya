//! The API contract: the JSON the routes accept and answer with, as typed
//! structures. The web client's types are generated from them
//! (`apps/web/lib/api/contract.ts`, ADR-036), so that both change together.
//! A request body is strict: an unknown field is refused, not ignored.

mod account_links;
pub mod admin;
mod approval_delegation;
mod approvals;
mod common;
mod conversations;
mod documents;
mod errors;
mod gateways;
mod health;
mod inbound_hooks;
mod links;
mod memory;
mod missions;
mod notifications;
mod run_stream;
mod runs;
mod scheduled_tasks;
mod session;
mod sso;
mod stored;
mod typescript;

pub use account_links::{AccountLinkStart, AccountLinkView, AccountLinks, LinkedAccount};
pub use approval_delegation::{Absence, ApprovalDelegations, Delegation};
pub use approvals::{Approval, ApprovalDecided, ApprovalList, ApprovalPerson, ApprovalVerdict};
pub use common::{Created, Done};
pub use conversations::{
    Citation, ConversationChanges, ConversationCreated, ConversationDetail, ConversationList,
    ConversationSummary, HistorySummary, Message, MessageAccepted, MessageMetadata, MessageRole,
    MissionPart, NewConversation, NewMessage,
};
pub use documents::{
    DocumentCreated, DocumentItem, DocumentKind, DocumentList, DocumentQuota, DocumentScope,
    DocumentSpaceOption, DocumentUpload,
};
pub use errors::{ErrorBody, ErrorDetail, ErrorKind};
pub use gateways::{
    AccountGatewayView, AccountGateways, GatewayIdentity, GatewayLinkCode, GatewayLinkPending,
    GatewayLinkPreview, TelegramLink,
};
pub use health::Health;
pub use inbound_hooks::{
    DeliveryOutcome, InboundHook, InboundHookList, InboundHookToken, InboundHookUpdate,
    LastDelivery, NewInboundHook, hook_path, hook_url,
};
pub use links::{InvitationDetails, LinkPassword, LinkToken, ResetDetails, ResetLinkRequest};
pub use memory::{
    ManualMemory, ManualMemoryKind, MemoryItem, MemoryKind, MemoryList, MemoryReview, MemoryScope,
    MemorySource, MemorySpace, MemoryStatus, ReviewDecision,
};
pub use missions::{Mission, MissionList, MissionStatus, NewMission};
pub use notifications::NotificationSettings;
pub use run_stream::{
    ApprovalRequestedEvent, ApprovalResolvedEvent, AssistantDelta, AssistantMessage,
    ConnectorsUnavailable, RunFinished, RunQueued, RunResumed, RunStarted, RunStreamEvent,
    ToolCallEvent, ToolDecision, ToolResultEvent, ToolRisk, UsageEvent, VerificationEvent,
};
pub use runs::{
    AgentProfile, RunBudget, RunDetail, RunEventRecord, RunList, RunListItem, RunRef, RunStatus,
    RunUsage, RunView, ToolInvocation, Verification,
};
pub use scheduled_tasks::{
    LastOccurrence, NewScheduledTask, OccurrenceOutcome, ScheduledTask, ScheduledTaskList,
    ScheduledTaskUpdate,
};
pub use session::{
    AssistantStatus, AuthOptions, Credentials, Me, MeTenant, MeUser, OrgUnitRef, PasswordUpdate,
};
pub use sso::{SsoDiscovery, SsoLink, SsoLinkLookup, SsoLookup, SsoProvider, SsoStart};

pub(crate) use stored::stored_text;
use typescript::Declarations;

/// The TypeScript module of the contract: every response and request body,
/// with the types they refer to.
pub fn typescript() -> String {
    let mut declarations = Declarations::new();
    declarations.add::<Me>();
    declarations.add::<AuthOptions>();
    declarations.add::<SsoDiscovery>();
    declarations.add::<SsoLink>();
    declarations.add::<SsoStart>();
    declarations.add::<Done>();
    declarations.add::<AccountLinks>();
    declarations.add::<AccountLinkStart>();
    declarations.add::<AccountGateways>();
    declarations.add::<GatewayLinkPreview>();
    declarations.add::<TelegramLink>();
    declarations.add::<ConversationList>();
    declarations.add::<ConversationCreated>();
    declarations.add::<ConversationDetail>();
    declarations.add::<MessageAccepted>();
    declarations.add::<RunList>();
    declarations.add::<RunDetail>();
    declarations.add::<ApprovalList>();
    declarations.add::<ApprovalDecided>();
    declarations.add::<ApprovalDelegations>();
    declarations.add::<RunStreamEvent>();
    declarations.add::<MemoryList>();
    declarations.add::<ScheduledTaskList>();
    declarations.add::<InboundHookList>();
    declarations.add::<InboundHookToken>();
    declarations.add::<MissionList>();
    declarations.add::<DocumentList>();
    declarations.add::<DocumentItem>();
    declarations.add::<DocumentCreated>();
    declarations.add::<Mission>();
    declarations.add::<Created>();
    declarations.add::<InvitationDetails>();
    declarations.add::<ResetDetails>();
    declarations.add::<Health>();
    declarations.add::<admin::Organization>();
    declarations.add::<admin::Directory>();
    declarations.add::<admin::UserCreated>();
    declarations.add::<admin::InvitationIssued>();
    declarations.add::<admin::TemporaryPassword>();
    declarations.add::<admin::ResetLinkSent>();
    declarations.add::<admin::CollaboratorAbsence>();
    declarations.add::<admin::AbsenceRecorded>();
    declarations.add::<admin::Roles>();
    declarations.add::<admin::SkillList>();
    declarations.add::<admin::SkillDetail>();
    declarations.add::<admin::SkillVersionCreated>();
    declarations.add::<admin::SkillEvaluations>();
    declarations.add::<admin::SkillEvaluationDetail>();
    declarations.add::<admin::Policies>();
    declarations.add::<admin::KnowledgeShares>();
    declarations.add::<admin::ApprovalRoutingView>();
    declarations.add::<admin::LlmView>();
    declarations.add::<admin::LlmTestResult>();
    declarations.add::<admin::EmbeddingView>();
    declarations.add::<admin::EmbeddingTestResult>();
    declarations.add::<admin::MemorySettingsView>();
    declarations.add::<admin::MissionSettingsView>();
    declarations.add::<admin::IdentityProvidersView>();
    declarations.add::<admin::Connectors>();
    declarations.add::<admin::Gateways>();
    // Named by `ConnectorView.toolPolicies`, which holds the stored value.
    declarations.add::<admin::ToolPolicy>();
    declarations.add::<admin::ConnectorTest>();
    declarations.add::<admin::RestImport>();
    declarations.add::<admin::AuditLog>();
    declarations.add::<admin::UsageSummary>();
    declarations.add::<ErrorBody>();
    macro_rules! declare {
        ($($body:ty),* $(,)?) => { $(declarations.add::<$body>();)* };
    }
    request_bodies!(declare);
    declarations.render()
}

/// Every request body (ADR-039, ADR-040), for `typescript()` and for the test that
/// each one refuses an unknown field: `$apply!(Type, …)`.
macro_rules! request_bodies {
    ($apply:ident) => {
        $apply!(
            Credentials,
            SsoLookup,
            SsoLinkLookup,
            PasswordUpdate,
            NotificationSettings,
            GatewayLinkCode,
            ResetLinkRequest,
            LinkToken,
            LinkPassword,
            NewConversation,
            ConversationChanges,
            NewMessage,
            ApprovalVerdict,
            Absence,
            NewScheduledTask,
            ScheduledTaskUpdate,
            NewInboundHook,
            InboundHookUpdate,
            NewMission,
            ManualMemory,
            MemoryReview,
            DocumentUpload,
            admin::NewAccount,
            admin::AccountUpdate,
            admin::RoleGrant,
            admin::NewOrgUnit,
            admin::OrgUnitUpdate,
            admin::NewRole,
            admin::RoleUpdate,
            admin::NewPolicy,
            admin::PolicyUpdate,
            admin::NewKnowledgeShare,
            admin::NewSkill,
            admin::SkillUpdate,
            admin::NewSkillVersion,
            admin::SkillActivation,
            admin::SkillRejection,
            admin::SkillEvalCaseInput,
            admin::EvaluationRequest,
            admin::NewConnector,
            admin::NewRestConnector,
            admin::NewMicrosoft365Connector,
            admin::NewGoogleWorkspaceConnector,
            admin::NewSlackGateway,
            admin::NewTeamsGateway,
            admin::NewTelegramGateway,
            admin::GatewayUpdate,
            admin::OpenApiDescription,
            admin::ConnectorTrial,
            admin::ConnectorUpdate,
            admin::LlmChoice,
            admin::EmbeddingChoice,
            admin::MemorySettingsUpdate,
            admin::MissionSettingsUpdate,
            admin::NewIdentityProvider,
            admin::IdentityProviderUpdate,
            admin::DomainProofRequest,
            admin::ApprovalRoutingUpdate
        )
    };
}
use request_bodies;

#[cfg(test)]
mod tests;
