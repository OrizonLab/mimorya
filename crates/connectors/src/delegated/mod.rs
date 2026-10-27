//! What the connectors whose tools act as the person a run serves share:
//! Microsoft 365 (ADR-117) and Google Workspace (ADR-122). Their provider's
//! API reached with that person's token, bounded (`api`); the token taken
//! from the platform and the provider's refusals said in French (`reach`);
//! the few bounded fields kept of its answers (`shape`).

mod api;
mod reach;
pub(crate) mod shape;

pub(crate) use api::ProviderApi;
pub(crate) use reach::{AsPerson, read_as_person, write_as_person};
