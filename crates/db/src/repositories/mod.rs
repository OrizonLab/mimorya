//! Repositories, one module per entity. Tenant data is read and written in a
//! `TenantTx` (row-level security applies); only the `auth_*`/`system_*`
//! functions take the raw pool.

pub mod access_policies;
pub mod approval_notices;
pub mod approval_settings;
pub mod approval_views;
pub mod approvals;
pub mod audit;
pub mod auth;
pub mod connectors;
pub mod conversations;
pub mod deliverables;
pub mod invitations;
pub mod llm_settings;
pub mod memory;
pub mod org;
pub mod org_units;
pub mod password_resets;
pub mod role_assignments;
pub mod roles;
pub mod run_events;
pub mod runs;
pub mod secrets;
pub mod skills;
pub mod tenants;
pub mod tool_invocations;
pub mod usage;
pub mod users;
