//! The values a `CHECK` constraint allows are those of the list the platform
//! writes them from: the API contract reads these columns as enumerations
//! built from the same lists (ADR-037), so a value added to one side only
//! fails here rather than as an internal error on a screen.

mod support;

use mimorya_core::authz::Sensitivity;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::memory::{MemoryKind, MemoryScope, MemoryStatus};
use mimorya_db::repositories::approvals::APPROVAL_STATUSES;
use mimorya_db::repositories::runs::RunStatus;
use mimorya_db::repositories::skills::SKILL_STATUSES;
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
    ];
    for (constraint, values) in expected {
        assert_eq!(allowed(&db, constraint).await, values, "{constraint}");
    }
}
