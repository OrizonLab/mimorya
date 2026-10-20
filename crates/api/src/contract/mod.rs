//! The API contract: the JSON the routes accept and answer with, as typed
//! structures. The web client's types are generated from them
//! (`apps/web/lib/api/contract.ts`, ADR-036), so that both change together.
//! A request body is strict: an unknown field is refused, not ignored.

pub mod admin;
mod approvals;
mod common;
mod conversations;
mod errors;
mod health;
mod links;
mod memory;
mod run_stream;
mod runs;
mod session;
mod stored;
mod typescript;

pub use approvals::{Approval, ApprovalDecided, ApprovalList, ApprovalVerdict};
pub use common::{Created, Done};
pub use conversations::{
    Citation, ConversationChanges, ConversationCreated, ConversationDetail, ConversationList,
    ConversationSummary, Message, MessageAccepted, MessageMetadata, MessageRole, NewConversation,
    NewMessage,
};
pub use errors::{ErrorBody, ErrorDetail, ErrorKind};
pub use health::Health;
pub use links::{InvitationDetails, LinkPassword, LinkToken, ResetDetails, ResetLinkRequest};
pub use memory::{
    ManualMemory, ManualMemoryKind, MemoryItem, MemoryKind, MemoryList, MemoryReview, MemoryScope,
    MemorySource, MemorySpace, MemoryStatus, ReviewDecision,
};
pub use run_stream::{
    ApprovalRequestedEvent, ApprovalResolvedEvent, AssistantDelta, AssistantMessage,
    ConnectorsUnavailable, RunFinished, RunStarted, RunStreamEvent, ToolCallEvent, ToolDecision,
    ToolResultEvent, ToolRisk, UsageEvent, VerificationEvent,
};
pub use runs::{
    RunBudget, RunDetail, RunEventRecord, RunList, RunListItem, RunRef, RunStatus, RunUsage,
    RunView, ToolInvocation, Verification,
};
pub use session::{
    AssistantStatus, AuthOptions, Credentials, Me, MeTenant, MeUser, OrgUnitRef, PasswordUpdate,
};

pub(crate) use stored::stored_text;
use typescript::Declarations;

/// The TypeScript module of the contract: every response and request body,
/// with the types they refer to.
pub fn typescript() -> String {
    let mut declarations = Declarations::new();
    declarations.add::<Me>();
    declarations.add::<AuthOptions>();
    declarations.add::<Done>();
    declarations.add::<ConversationList>();
    declarations.add::<ConversationCreated>();
    declarations.add::<ConversationDetail>();
    declarations.add::<MessageAccepted>();
    declarations.add::<RunList>();
    declarations.add::<RunDetail>();
    declarations.add::<ApprovalList>();
    declarations.add::<ApprovalDecided>();
    declarations.add::<RunStreamEvent>();
    declarations.add::<MemoryList>();
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
    declarations.add::<admin::Roles>();
    declarations.add::<admin::SkillList>();
    declarations.add::<admin::Policies>();
    declarations.add::<admin::LlmView>();
    declarations.add::<admin::LlmTestResult>();
    declarations.add::<admin::Connectors>();
    // Named by `ConnectorView.toolPolicies`, which holds the stored value.
    declarations.add::<admin::ToolPolicy>();
    declarations.add::<admin::ConnectorTest>();
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
            PasswordUpdate,
            ResetLinkRequest,
            LinkToken,
            LinkPassword,
            NewConversation,
            ConversationChanges,
            NewMessage,
            ApprovalVerdict,
            ManualMemory,
            MemoryReview,
            admin::NewAccount,
            admin::AccountUpdate,
            admin::RoleGrant,
            admin::NewOrgUnit,
            admin::OrgUnitUpdate,
            admin::NewRole,
            admin::RoleUpdate,
            admin::NewPolicy,
            admin::PolicyUpdate,
            admin::NewConnector,
            admin::ConnectorTrial,
            admin::ConnectorUpdate,
            admin::LlmChoice
        )
    };
}
use request_bodies;

#[cfg(test)]
mod tests;
