//! Pure domain of the platform: authorization (scoped RBAC + ABAC), the
//! organisation tree, autonomy levels, the tool contract, memory visibility
//! rules and the fictional demo company. No I/O.

pub mod administration;
pub mod authz;
pub mod autonomy;
pub mod custom_roles;
pub mod demo;
pub mod error;
pub mod memory;
pub mod roles;
pub mod tools;

pub use error::{DomainError, ErrorCode};
