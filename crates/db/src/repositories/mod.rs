//! Repositories, one module per entity. Tenant data is read and written in a
//! `TenantTx` (row-level security applies); only the `auth_*`/`system_*`
//! functions take the raw pool.

pub mod access_policies;
pub mod approval_delegations;
pub mod approval_notices;
pub mod approval_settings;
pub mod approval_views;
pub mod approvals;
pub mod audit;
pub mod auth;
pub mod author_notices;
pub mod connectors;
pub mod conversations;
pub mod deliverables;
pub mod embedding_settings;
pub mod identity_providers;
pub mod invitations;
pub mod llm_settings;
pub mod memory;
pub mod memory_embeddings;
pub mod memory_search;
pub mod memory_settings;
pub mod mission_end_notices;
pub mod mission_settings;
pub mod missions;
pub mod notification_settings;
pub mod org;
pub mod org_units;
pub mod password_resets;
pub mod rate_limits;
pub mod role_assignments;
pub mod roles;
pub mod run_events;
pub mod run_leases;
pub mod run_queue;
pub mod runs;
pub mod scheduled_tasks;
pub mod secrets;
pub mod skills;
pub mod sso_assertions;
pub mod tenants;
pub mod tool_invocations;
pub mod usage;
pub mod user_identities;
pub mod users;
pub mod verified_domains;
