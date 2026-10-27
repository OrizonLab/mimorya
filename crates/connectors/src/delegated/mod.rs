//! What the connectors whose tools act as the person a run serves share:
//! Microsoft 365 (ADR-117) and Google Workspace (ADR-122). Their provider's
//! API reached with that person's token, bounded (`api`); the token taken
//! from the platform and the provider's refusals said in French (`reach`);
//! the few bounded fields kept of its answers (`shape`); and what their
//! tools write in the person's name, as the model asks it and the person
//! validates it, hiding nothing (`compose`, `new_mail`, `new_event`,
//! ADR-119, ADR-123).

mod api;
mod compose;
mod new_event;
mod new_mail;
mod reach;
pub(crate) mod shape;

pub(crate) use api::ProviderApi;
pub(crate) use new_event::NewEvent;
pub(crate) use new_mail::NewMail;
pub(crate) use reach::{AsPerson, read_as_person, write_as_person};
