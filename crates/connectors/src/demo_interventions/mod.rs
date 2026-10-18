//! Demo connector « Interventions » over a FICTIONAL dataset.

mod access;
mod anomalies;
mod connector;
pub mod dataset;
pub mod dates;
mod directory;
mod schedule;
mod schemas;
mod store;
mod tools;

pub use connector::{DemoInterventionsConnector, demo_intervention_tools};
pub use schemas::ProposedSlot;
pub use store::{DemoInterventionsStore, ScheduledSlot};
