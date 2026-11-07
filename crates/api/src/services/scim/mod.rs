//! SCIM 2.0 provisioning of the company's accounts by its identity provider
//! (ADR-183): tokens made, listed and removed by who manages the company's
//! accounts (`tokens.rs`), the company and token a request comes with
//! (`caller.rs`), and the accounts read and created (`users.rs`).

mod caller;
mod tokens;
mod users;

pub use caller::authenticate;
pub use tokens::{SCIM_TOKENS, TOKEN_PREFIX};
pub use users::{ScimPage, create_scim_user, list_scim_users, read_scim_user};
