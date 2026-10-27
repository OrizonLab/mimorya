//! Workspace administration: accounts and the absence of a collaborator who
//! cannot declare it, organisation, custom roles, role assignments, access
//! policies, skills, their evaluation cases and evaluations, the company's
//! model provider and its identity providers.
//! Every operation requires its permission on the whole company, refuses to
//! act on the administrator's own account, runs in one transaction with its
//! audit entry, and refreshes the cached authorization data of the tenant.
//!
//! Changes of one company are serialised (`begin_change`): each one checks
//! its rules on the data as stored, not on the request's cached view, so two
//! administrators acting at once cannot build a cycle in the tree or revoke
//! each other's rights.

mod absence_mail;
mod absences;
mod approval_routing;
mod assignments;
mod connector_config;
mod connector_rules;
mod connector_test;
mod connector_trial;
mod connectors;
mod domain_proofs;
mod embedding;
mod google_workspace;
mod identity_providers;
mod invitations;
mod link_sender;
mod llm;
mod memory_settings;
mod microsoft365;
mod mission_settings;
mod org_units;
mod policies;
mod provider_designation;
mod reset_links;
mod rest_connectors;
mod roles;
mod skill_eval_cases;
mod skill_evaluations;
mod skill_proposals;
mod skills;
mod users;

pub use absences::{AbsenceAdmin, absence_of, declare_absence_for, end_absence_for};
pub use approval_routing::{manager_risks, set_manager_risks};
pub use assignments::{grant_role, revoke_role};
pub use connector_config::{Editable, RestChanges};
pub use connector_rules::ToolPolicyInput;
pub use connector_test::{ConnectorTestOutcome, TestedTool, test_connector};
pub use connector_trial::{McpTrial, TrialServices, try_mcp_connector};
pub use connectors::{
    ConnectorAdmin, ConnectorChanges, NewMcpConnector, TokenChange, create_mcp_connector,
    delete_connector, update_connector,
};
pub use domain_proofs::prove_domain;
pub use embedding::{
    EmbeddingInput, EmbeddingTestOutcome, configure_embedding, reset_embedding, test_embedding,
};
pub use google_workspace::{NewGoogleWorkspace, create_google_workspace_connector};
pub use identity_providers::{
    ProviderChanges, ProviderDeclaration, change_identity_provider, declare_identity_provider,
    remove_identity_provider,
};
pub use invitations::{IssuedInvitation, issue_invitation};
pub use link_sender::LinkSender;
pub use llm::{LlmInput, LlmTestOutcome, configure_llm, reset_llm, test_llm};
pub use memory_settings::{lesson_capture, set_lesson_capture};
pub use microsoft365::{NewMicrosoft365, create_microsoft365_connector};
pub use mission_settings::{
    company_mission_limits, reset_company_mission_limits, set_company_mission_limits,
};
pub use org_units::{NewUnitInput, UnitChanges, create_unit, delete_unit, update_unit};
pub use policies::{PolicyChanges, PolicyInput, create_policy, delete_policy, update_policy};
pub use reset_links::send_reset_link;
pub use rest_connectors::{NewRestConnector, create_rest_connector, read_openapi};
pub use roles::{RoleChanges, RoleInput, create_role, delete_role, update_role};
pub use skill_eval_cases::{EvalCaseInput, change_eval_case, create_eval_case, remove_eval_case};
pub use skill_evaluations::launch_evaluation;
pub use skill_proposals::reject_skill_proposal;
pub use skills::{
    SkillInput, VersionInput, activate_skill, add_skill_version, create_skill, delete_skill,
    retire_skill, update_skill,
};
pub use users::{NewUserInput, UserChanges, create_user, reset_password, update_user};

use mimorya_core::DomainError;
use mimorya_core::authz::{Actor, AuthorizationEngine, DataScope, OrgTree};
use mimorya_db::repositories::audit::AuditEntry;
use mimorya_db::repositories::org::{
    AuthorizationData, load_authorization_data, load_org_units, load_user_access,
};
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use crate::services::AccessContext;

/// Opens the transaction of a change under the company's administration lock,
/// once the administrator still holds `permission` company-wide on the stored
/// data: their account and assignments, and the roles and policies themselves
/// (another administrator may have revoked, disabled or narrowed them
/// meanwhile; the request's cached view would not show it).
async fn begin_change(
    db: &Database,
    access: &AccessContext,
    permission: &str,
) -> anyhow::Result<TenantTx> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    tx.lock_administration().await?;
    let engine = engine_of(load_authorization_data(&mut tx).await?)?;
    if !holds_company_wide(&mut tx, &engine, access.subject.user_id, permission).await? {
        return Err(DomainError::forbidden(format!(
            "Permission requise sur toute l’entreprise : {permission}"
        ))
        .into());
    }
    Ok(tx)
}

/// The engine of the company's rules as stored: its units, roles and
/// policies.
pub(crate) fn engine_of(data: AuthorizationData) -> anyhow::Result<AuthorizationEngine> {
    let definitions = data.roles.into_iter().map(|r| r.definition).collect();
    Ok(AuthorizationEngine::new(
        OrgTree::new(data.units)?,
        definitions,
        data.policies,
    ))
}

/// Whether `user_id` holds `permission` on the whole company as the stored
/// data say now: an active account, its assignments, and `engine`'s rules,
/// deny policies included. What an administration change requires
/// (`begin_change`), and what tells the skills' administrators apart
/// (ADR-102).
pub(crate) async fn holds_company_wide(
    tx: &mut TenantTx,
    engine: &AuthorizationEngine,
    user_id: Uuid,
    permission: &str,
) -> anyhow::Result<bool> {
    let current = load_user_access(tx, user_id).await?;
    Ok(current
        .filter(|a| a.profile.status == "active")
        .is_some_and(|a| {
            engine.data_scope(&a.subject, permission, &Actor::user()) == DataScope::All
        }))
}

/// The organisation tree as stored now (read under the administration lock).
async fn current_tree(tx: &mut TenantTx) -> anyhow::Result<OrgTree> {
    Ok(OrgTree::new(load_org_units(tx).await?)?)
}

fn audit_entry(
    access: &AccessContext,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
) -> AuditEntry {
    AuditEntry::new(access.subject.tenant_id, "user", action, "success")
        .by_user(access.subject.user_id)
        .resource(resource_type, resource_id)
}

/// Administrators do not change their own account, rights or status: it
/// takes another administrator (no self-escalation, no self-lockout).
fn refuse_self(access: &AccessContext, user_id: Uuid) -> Result<(), DomainError> {
    if access.subject.user_id == user_id {
        return Err(DomainError::forbidden(
            "Vous ne pouvez pas modifier votre propre compte ni vos propres habilitations depuis l’administration",
        ));
    }
    Ok(())
}
