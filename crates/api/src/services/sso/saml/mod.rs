//! SAML 2.0 providers (ADR-068): their declared metadata, the request the
//! browser carries to them, the verification of the response it posts
//! back, and what a verified assertion says, for the same rules as an
//! OpenID Connect token.

mod descriptor;
mod flow;
mod pending;
mod statement;

pub use descriptor::summary;
pub use flow::{Carried, ServiceProvider, authn_request, sp_metadata, verify_response};
pub use pending::SamlPending;
pub use statement::Statement;
