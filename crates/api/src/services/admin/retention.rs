//! How long the company keeps its idle conversations, its episodes and the
//! personal memories of its accounts gone (ADR-195), read and set by those
//! who hold `retention:manage` company-wide, within the platform's bounds.
//! A change is audited with what it was and what it is; a shortening waits
//! a week before the purge applies it, which the database decides. A
//! duration asked as it already is changes nothing, a shortening that
//! waits included.

use chrono::{DateTime, Utc};
use mimorya_core::retention::RetentionKind;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::retention_durations::{
    StoredRetention, retention_durations, set_retention_duration,
};
use serde_json::{Value, json};

use super::{audit_entry, begin_change};
use crate::services::AccessContext;

const PERMISSION: &str = "retention:manage";

/// One kind's duration, as the company has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionDuration {
    pub kind: RetentionKind,
    /// The company's choice, or the platform's without one.
    pub duration: i32,
    /// What the purge applies now.
    pub in_effect: i32,
    pub own: bool,
    /// When a shortening that waits applies.
    pub shortening_applies_at: Option<DateTime<Utc>>,
}

impl RetentionDuration {
    fn of(kind: RetentionKind, stored: &[StoredRetention]) -> Self {
        match stored.iter().find(|row| row.kind == kind.key()) {
            Some(row) => Self {
                kind,
                duration: row.duration,
                in_effect: row.in_effect,
                own: true,
                shortening_applies_at: (row.in_effect > row.duration)
                    .then_some(row.shortening_applies_at),
            },
            None => Self {
                kind,
                duration: kind.platform(),
                in_effect: kind.platform(),
                own: false,
                shortening_applies_at: None,
            },
        }
    }
}

/// The company's duration of each kind.
pub async fn company_retention(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<Vec<RetentionDuration>> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let stored = retention_durations(&mut tx).await?;
    tx.commit().await?;
    Ok(RetentionKind::ALL
        .into_iter()
        .map(|kind| RetentionDuration::of(kind, &stored))
        .collect())
}

/// Sets the company's durations, each within the platform's bounds; those
/// asked as they already are stay as they are. Audited once, with each
/// change, if any.
pub async fn set_company_retention(
    db: &Database,
    access: &AccessContext,
    asked: [(RetentionKind, i32); 3],
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    for (kind, duration) in asked {
        kind.check(duration)?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let stored = retention_durations(&mut tx).await?;
    let mut changes = serde_json::Map::new();
    for (kind, duration) in asked {
        let current = RetentionDuration::of(kind, &stored);
        if current.duration == duration {
            continue;
        }
        set_retention_duration(
            &mut tx,
            tenant_id,
            kind.key(),
            duration,
            access.subject.user_id,
        )
        .await?;
        changes.insert(kind.key().to_string(), change(&current, duration));
    }
    if !changes.is_empty() {
        append_audit(
            &mut tx,
            audit_entry(access, "retention.configured", "tenant", tenant_id)
                .metadata(Value::Object(changes)),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// What a change audits: the duration it was and the one it is, and
/// whether the purge waits a week to apply it, shorter than what it
/// applies now.
fn change(current: &RetentionDuration, duration: i32) -> Value {
    json!({
        "from": current.duration,
        "to": duration,
        "delayed": duration < current.in_effect,
    })
}
