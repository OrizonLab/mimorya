//! SCIM 2.0 (RFC 7643, RFC 7644) as a company's identity provider speaks it
//! to provision its accounts (ADR-183): what a filter may ask, what an
//! account sent may hold and what is kept of it. No I/O.

mod changes;
mod filter;
mod user;

pub use changes::{ChangeRefused, ScimChanges, patch, replacement};
pub use filter::{UserFilter, parse_user_filter};
pub use user::{
    EXTERNAL_ID_MAX_CHARS, NewScimUser, ScimUserFields, TITLE_MAX_CHARS, read_new_user,
};
