//! What the connectors whose tools act as the person a run serves share:
//! Microsoft 365 (ADR-117) and Google Workspace (ADR-122). Their provider's
//! API reached with that person's token, bounded (`api`); the token taken
//! from the platform and the provider's refusals said in French (`reach`);
//! the few bounded fields kept of its answers (`shape`).

pub mod api;
pub mod reach;
pub mod shape;

pub use api::{ApiFailure, MAX_ANSWER_BYTES, ProviderApi};
pub use reach::{AsPerson, read_as_person, write_as_person};
