//! Pure domain of the platform: authorization (scoped RBAC + ABAC), the
//! organisation tree, autonomy levels, the tool contract, memory visibility
//! rules and the fictional demo company. No I/O.

pub mod accounts;
pub mod administration;
pub mod approval_routing;
pub mod authz;
pub mod autonomy;
pub mod collaborator_attributes;
pub mod custom_roles;
pub mod demo;
pub mod documents;
pub mod error;
pub mod identity_providers;
pub mod memory;
pub mod plain_text;
pub mod policy_rules;
pub mod provisioning;
pub mod roles;
pub mod schedule;
pub mod skill_proposals;
pub mod skill_rules;
pub mod tools;

pub use error::{DomainError, ErrorCode, UnknownReference};
