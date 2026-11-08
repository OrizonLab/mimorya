//! What a person's export reads of them (ADR-187), in one read-only
//! snapshot of their company: every read sees the same instant, so the
//! messages counted are those of the conversations exported, and a run's
//! tool calls those of the runs exported.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_db::TenantTx;
use mimorya_db::repositories::notification_settings::notification_settings;
use mimorya_db::repositories::personal_access::{
    exported_provisioning, exported_sessions, exported_sign_ins,
};
use mimorya_db::repositories::personal_activity::{
    exported_approvals, exported_decisions, exported_deliverables, exported_runs,
    exported_tool_calls, exported_usage,
};
use mimorya_db::repositories::personal_content::{
    exported_conversations, exported_memories, exported_messages, exported_missions,
    exported_summaries, exported_tasks, exported_triggers,
};
use mimorya_db::repositories::personal_data::{
    exported_absences, exported_authorizations, exported_identities, exported_links,
    exported_profile, exported_roles, exported_units,
};
use uuid::Uuid;

use crate::contract::{
    ExportAccount, NotificationSettings, PersonalExport, conversation_exports, run_exports,
};
use crate::http::iso;

const NO_ACCOUNT: &str = "Compte introuvable";

/// Everything `user_id`'s export holds, read in `snapshot`.
pub(super) async fn read_export(
    snapshot: &mut TenantTx,
    user_id: Uuid,
) -> anyhow::Result<PersonalExport> {
    let exported_at = iso(&Utc::now());
    let profile = exported_profile(snapshot, user_id)
        .await?
        .ok_or_else(|| DomainError::not_found(NO_ACCOUNT))?;
    let units = exported_units(snapshot, user_id).await?;
    let roles = exported_roles(snapshot, user_id).await?;
    let chosen = notification_settings(snapshot, user_id).await?;
    let conversations = conversation_exports(
        exported_conversations(snapshot, user_id).await?,
        exported_summaries(snapshot, user_id).await?,
        exported_messages(snapshot, user_id).await?,
    );
    let runs = run_exports(
        exported_runs(snapshot, user_id).await?,
        exported_tool_calls(snapshot, user_id).await?,
    );
    Ok(PersonalExport {
        exported_at,
        account: ExportAccount::new(profile, units, roles),
        notifications: NotificationSettings {
            approvals: chosen.approvals,
            own_runs: chosen.own_runs,
            mission_ends: chosen.mission_ends,
        },
        sessions: into_all(exported_sessions(snapshot, user_id).await?),
        sign_ins: into_all(exported_sign_ins(snapshot, user_id).await?),
        provisioning: into_all(exported_provisioning(snapshot, user_id).await?),
        conversations,
        memories: into_all(exported_memories(snapshot, user_id).await?),
        scheduled_tasks: into_all(exported_tasks(snapshot, user_id).await?),
        triggers: into_all(exported_triggers(snapshot, user_id).await?),
        missions: into_all(exported_missions(snapshot, user_id).await?),
        deliverables: into_all(exported_deliverables(snapshot, user_id).await?),
        runs,
        approvals: into_all(exported_approvals(snapshot, user_id).await?),
        decisions: into_all(exported_decisions(snapshot, user_id).await?),
        usage: into_all(exported_usage(snapshot, user_id).await?),
        absences: into_all(exported_absences(snapshot, user_id).await?),
        linked_accounts: into_all(exported_links(snapshot, user_id).await?),
        messaging_identities: into_all(exported_identities(snapshot, user_id).await?),
        server_authorizations: into_all(exported_authorizations(snapshot, user_id).await?),
    })
}

fn into_all<R, T: From<R>>(rows: Vec<R>) -> Vec<T> {
    rows.into_iter().map(T::from).collect()
}
