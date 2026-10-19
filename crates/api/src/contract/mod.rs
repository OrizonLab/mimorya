//! The API contract: the JSON the routes answer with, as typed structures.
//! The web client's types are generated from them
//! (`apps/web/lib/api/contract.ts`, ADR-036), so that both change together.

mod approvals;
mod conversations;
mod health;
mod links;
mod memory;
mod run_stream;
mod runs;
mod session;
mod stored;
mod typescript;

pub use approvals::{Approval, ApprovalDecided, ApprovalList};
pub use conversations::{
    Citation, ConversationCreated, ConversationDetail, ConversationList, ConversationSummary,
    Message, MessageAccepted, MessageMetadata, MessageRole,
};
pub use health::Health;
pub use links::{InvitationDetails, ResetDetails};
pub use memory::{
    MemoryCreated, MemoryItem, MemoryKind, MemoryList, MemoryScope, MemorySource, MemoryStatus,
};
pub use run_stream::{
    ApprovalRequestedEvent, ApprovalResolvedEvent, AssistantDelta, AssistantMessage, RunFinished,
    RunStarted, RunStreamEvent, ToolCallEvent, ToolDecision, ToolResultEvent, ToolRisk, UsageEvent,
    VerificationEvent,
};
pub use runs::{
    RunBudget, RunDetail, RunEventRecord, RunList, RunListItem, RunRef, RunStatus, RunUsage,
    RunView, ToolInvocation, Verification,
};
pub use session::{AssistantStatus, AuthOptions, Done, Me, MeTenant, MeUser, OrgUnitRef};

use typescript::Declarations;

/// The TypeScript module of the contract: every response, with the types
/// it refers to.
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
    declarations.add::<MemoryCreated>();
    declarations.add::<InvitationDetails>();
    declarations.add::<ResetDetails>();
    declarations.add::<Health>();
    declarations.render()
}

#[cfg(test)]
mod tests;
