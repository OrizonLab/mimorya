//! Connectors: each one brings tools that filter their data with the caller's
//! bound authorization. The demo connector serves FICTIONAL data.

pub mod demo_interventions;
pub mod mcp;
pub mod registry;
#[cfg(feature = "testing")]
pub mod testing;
pub mod types;

pub use registry::{connector_type, connector_types};
pub use types::{Connector, ConnectorCloser, ConnectorInfo, ConnectorRuntime, InstanceContext};
