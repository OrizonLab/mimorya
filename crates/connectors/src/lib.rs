//! Connectors: each one brings tools that filter their data with the caller's
//! bound authorization. The demo connector serves FICTIONAL data; MCP
//! servers and REST APIs are those a company connects itself (`outbound`);
//! Microsoft 365 and Google Workspace reach each person's own mailbox with
//! the access they granted (`microsoft365`, `google_workspace`).

pub mod delegated;
pub mod demo_interventions;
pub mod google_workspace;
pub mod mcp;
pub mod microsoft365;
pub mod outbound;
pub mod registry;
pub mod rest;
#[cfg(feature = "testing")]
pub mod testing;
pub mod types;

pub use registry::{connector_type, connector_types};
pub use types::{
    Connector, ConnectorCloser, ConnectorInfo, ConnectorRuntime, DEFAULT_CALL_TIMEOUT,
    DEFAULT_START_TIMEOUT, InstanceContext,
};
