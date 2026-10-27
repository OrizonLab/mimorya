//! Tool contract: what connectors and platform tools implement, and what the
//! agent's policy enforcement point evaluates.

mod accounts;
mod contract;
mod typed;

pub use accounts::{AccountAccessError, LinkedAccounts, NoLinkedAccounts};
pub use contract::{
    BoundAuthorization, Citation, Deliverable, DeliverableInput, DeliverableService,
    DeliverableStatus, Tool, ToolContext, ToolInfo, ToolRef, ToolResult, ToolServices,
    is_valid_tool_name,
};
pub use typed::{TypedTool, input_schema, typed_tool, validation_issues};
