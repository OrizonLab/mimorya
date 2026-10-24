//! The e-mails each person chooses to receive from the platform (ADR-094):
//! the requests awaiting their validation, as a manager or standing in for
//! one, with the reminders before they expire; their scheduled tasks' runs
//! and missions' stages awaiting it; the end of their missions. All of
//! them until they choose otherwise; invitations, password resets and an
//! absence an administrator declares for them are always sent. A notice
//! its recipient chose not to receive is not sent, and audited as refused
//! (`OPTED_OUT`, `optedOut` in its metadata, `deliver`): it counts as told,
//! and is not tried again. Each change is audited.

use std::future::Future;

use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::notification_settings::{
    NotificationSettingsRow, notification_settings, set_notification_settings,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::services::AccessContext;

/// A kind of e-mail one may choose not to receive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    Approvals,
    OwnRuns,
    MissionEnds,
}

/// The audit outcome of a notice its recipient chose not to receive.
const OPTED_OUT: &str = "denied";

/// What became of a notice: its audit outcome, and whether its recipient
/// chose not to receive it.
#[derive(Debug, Clone, Copy)]
pub struct Delivery {
    pub outcome: &'static str,
    opted_out: bool,
}

impl Delivery {
    /// The notice's audit `metadata`, marked when it was not wanted.
    pub fn mark(self, mut metadata: Value) -> Value {
        if self.opted_out {
            metadata["optedOut"] = json!(true);
        }
        metadata
    }
}

/// Sends `user_id`, of `tenant_id`, a notice of `notice` by `send`, which
/// gives its audit outcome, unless they chose not to receive these: then
/// nothing is sent, and its outcome is `OPTED_OUT`. Their choice is read
/// now, as the notice leaves.
pub async fn deliver(
    db: &Database,
    tenant_id: Uuid,
    user_id: Uuid,
    notice: Notice,
    send: impl Future<Output = &'static str>,
) -> anyhow::Result<Delivery> {
    if !wants(db, tenant_id, user_id, notice).await? {
        return Ok(Delivery {
            outcome: OPTED_OUT,
            opted_out: true,
        });
    }
    Ok(Delivery {
        outcome: send.await,
        opted_out: false,
    })
}

/// What the signed-in person chose.
pub async fn own_notification_settings(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<NotificationSettingsRow> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let chosen = notification_settings(&mut tx, access.subject.user_id).await?;
    tx.commit().await?;
    Ok(chosen)
}

/// Records what the signed-in person chooses.
pub async fn set_own_notification_settings(
    db: &Database,
    access: &AccessContext,
    chosen: NotificationSettingsRow,
) -> anyhow::Result<()> {
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    set_notification_settings(&mut tx, tenant_id, user_id, chosen).await?;
    let entry = AuditEntry::new(
        tenant_id,
        "user",
        "account.notifications_configured",
        "success",
    )
    .by_user(user_id)
    .resource("user", user_id)
    .metadata(json!({
        "approvals": chosen.approvals,
        "ownRuns": chosen.own_runs,
        "missionEnds": chosen.mission_ends,
    }));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

/// Whether `user_id`, of `tenant_id`, receives the e-mails of `notice`.
async fn wants(
    db: &Database,
    tenant_id: Uuid,
    user_id: Uuid,
    notice: Notice,
) -> anyhow::Result<bool> {
    let mut tx = db.begin(tenant_id).await?;
    let chosen = notification_settings(&mut tx, user_id).await?;
    tx.commit().await?;
    Ok(match notice {
        Notice::Approvals => chosen.approvals,
        Notice::OwnRuns => chosen.own_runs,
        Notice::MissionEnds => chosen.mission_ends,
    })
}
