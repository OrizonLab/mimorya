//! The values a `CHECK` constraint allows are exactly those of their
//! reference list (an enumeration of the core, or a constant of the
//! repositories), against which the API contract's enumerations are tested
//! (ADR-037): a value added to one side only fails here rather than as an
//! internal error on a screen.

mod support;

use mimorya_core::authz::Sensitivity;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::documents::{DocumentKind, DocumentSpace};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemoryStatus};
use mimorya_db::repositories::access_policies::POLICY_EFFECTS;
use mimorya_db::repositories::approvals::APPROVAL_STATUSES;
use mimorya_db::repositories::audit::{AUDIT_ACTORS, AUDIT_OUTCOMES};
use mimorya_db::repositories::connectors::CONNECTOR_STATUSES;
use mimorya_db::repositories::conversations::MESSAGE_ROLES;
use mimorya_db::repositories::runs::RunStatus;
use mimorya_db::repositories::skills::{SKILL_STATUSES, SKILL_VERSION_STATUSES};
use mimorya_db::repositories::tool_invocations::TOOL_DECISIONS;
use mimorya_db::repositories::usage::USAGE_KINDS;
use mimorya_db::repositories::users::USER_STATUSES;
use regex::Regex;

/// The quoted values of a constraint, in its order.
async fn allowed(db: &mimorya_db::Database, constraint: &str) -> Vec<String> {
    let definition: String = sqlx::query_scalar(
        "select pg_get_constraintdef(oid) from pg_constraint \
         where conname = $1 and contype = 'c'",
    )
    .bind(constraint)
    .fetch_one(db.pool())
    .await
    .unwrap_or_else(|e| panic!("constraint {constraint}: {e}"));
    let quoted = Regex::new(r"'([^']*)'").unwrap();
    quoted
        .captures_iter(&definition)
        .map(|c| c[1].to_string())
        .collect()
}

fn owned<I: IntoIterator<Item = &'static str>>(values: I) -> Vec<String> {
    values.into_iter().map(String::from).collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn constrained_columns_allow_exactly_the_platform_s_values() {
    let urls = support::urls(|_| {});
    let db = support::connect(&urls.owner).await;
    let expected = [
        ("users_status_check", owned(USER_STATUSES)),
        (
            "users_clearance_check",
            owned(Sensitivity::ALL.map(Sensitivity::as_str)),
        ),
        ("skills_status_check", owned(SKILL_STATUSES)),
        ("skill_versions_status_check", owned(SKILL_VERSION_STATUSES)),
        (
            "memory_items_scope_check",
            owned(MemoryScope::ALL.map(MemoryScope::as_str)),
        ),
        (
            "memory_items_kind_check",
            owned(MemoryKind::ALL.map(MemoryKind::as_str)),
        ),
        (
            "memory_items_status_check",
            owned(MemoryStatus::ALL.map(MemoryStatus::as_str)),
        ),
        (
            "approvals_risk_check",
            owned(ToolRisk::ALL.map(ToolRisk::as_str)),
        ),
        ("approvals_status_check", owned(APPROVAL_STATUSES)),
        (
            "agent_runs_status_check",
            owned(RunStatus::ALL.map(RunStatus::as_str)),
        ),
        ("audit_log_actor_check", owned(AUDIT_ACTORS)),
        ("audit_log_outcome_check", owned(AUDIT_OUTCOMES)),
        ("usage_records_kind_check", owned(USAGE_KINDS)),
        ("access_policies_effect_check", owned(POLICY_EFFECTS)),
        (
            "connector_instances_status_check",
            owned(CONNECTOR_STATUSES),
        ),
        ("messages_role_check", owned(MESSAGE_ROLES)),
        ("tool_invocations_decision_check", owned(TOOL_DECISIONS)),
        ("documents_scope_check", owned(DocumentSpace::SCOPES)),
        (
            "documents_kind_check",
            owned(DocumentKind::ALL.map(DocumentKind::as_str)),
        ),
        (
            "documents_sensitivity_check",
            owned(Sensitivity::ALL.map(Sensitivity::as_str)),
        ),
    ];
    for (constraint, values) in expected {
        assert_eq!(allowed(&db, constraint).await, values, "{constraint}");
    }
}
