//! How long the company keeps its idle conversations, its episodes and the
//! personal memories of its accounts gone (ADR-195), read and set by those
//! who hold `retention:manage` company-wide, within the platform's bounds.
//! A change is audited with what it was and what it is; a shortening waits
//! a week before the purge applies it, which the database decides. A
//! duration asked as it already is changes nothing, a shortening that
//! waits included. A change names the version of the durations it was
//! asked on, refused once another changed them since (ADR-196): what its
//! administrator was asked to confirm is no longer what it would change.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_core::retention::RetentionKind;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::retention_durations::{
    StoredRetention, retention_durations, set_retention_duration,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{audit_entry, begin_change};
use crate::services::AccessContext;

const PERMISSION: &str = "retention:manage";

/// A change asked on durations that another changed since.
pub const DURATIONS_CHANGED: &str = "Les durées de conservation ont changé depuis l’ouverture \
                                     de cette page : rechargez-la pour voir les durées actuelles";

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

/// The company's durations, as a change names them.
pub struct CompanyRetention {
    /// Each kind's, in `RetentionKind::ALL`'s order.
    pub durations: Vec<RetentionDuration>,
    /// The version of what the company set, which a change names.
    pub version: String,
}

/// The company's duration of each kind.
pub async fn company_retention(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<CompanyRetention> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let stored = retention_durations(&mut tx).await?;
    tx.commit().await?;
    Ok(CompanyRetention {
        durations: RetentionKind::ALL
            .into_iter()
            .map(|kind| RetentionDuration::of(kind, &stored))
            .collect(),
        version: version_of(&stored),
    })
}

/// Sets the company's durations, each within the platform's bounds; those
/// asked as they already are stay as they are. Refused once what the
/// company set is no longer the version `replaces` names. Audited once,
/// with each change, if any.
pub async fn set_company_retention(
    db: &Database,
    access: &AccessContext,
    asked: [(RetentionKind, i32); 3],
    replaces: &str,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    for (kind, duration) in asked {
        kind.check(duration)?;
    }
    let tenant_id = access.subject.tenant_id;
    // Under the company's administration lock: no other change of its
    // durations passes between this reading and the writing.
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let stored = retention_durations(&mut tx).await?;
    if version_of(&stored) != replaces {
        return Err(DomainError::conflict(DURATIONS_CHANGED).into());
    }
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

/// The version of what the company set: each kind's duration and when it
/// was chosen, which every change moves. What the purge applies as days
/// pass is not in it: it only shortens, so a form read before asks about
/// more shortenings than there are, never fewer.
fn version_of(stored: &[StoredRetention]) -> String {
    let set: Vec<Value> = stored
        .iter()
        .map(|row| json!([row.kind, row.duration, row.changed_at.timestamp_micros()]))
        .collect();
    format!("{:x}", Sha256::digest(Value::Array(set).to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(kind: &str, duration: i32, changed_at: DateTime<Utc>) -> StoredRetention {
        StoredRetention {
            kind: kind.to_string(),
            duration,
            in_effect: duration,
            changed_at,
            shortening_applies_at: changed_at,
        }
    }

    #[test]
    fn a_duration_or_its_date_changing_changes_the_version() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("date");
        let later = at + chrono::Duration::microseconds(1);
        let set = [stored("episodes", 6, at)];
        let version = version_of(&set);
        assert_eq!(version, version_of(&[stored("episodes", 6, at)]));
        assert_ne!(version, version_of(&[]));
        assert_ne!(version, version_of(&[stored("episodes", 7, at)]));
        assert_ne!(version, version_of(&[stored("episodes", 6, later)]));
        assert_ne!(version, version_of(&[stored("departed", 6, at)]));
        // What the purge applies as days pass is not in it.
        let mut waited = stored("episodes", 6, at);
        waited.in_effect = 12;
        assert_eq!(version, version_of(&[waited]));
        assert_eq!(version.len(), 64);
    }
}
