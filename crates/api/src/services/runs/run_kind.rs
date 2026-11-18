//! What kind of run a run is, as the rules of a company's model router
//! tell them apart (ADR-207): a mission's stage, a hook's run, a scheduled
//! task's, or else a person's turn of conversation, whatever its channel.

use mimorya_agent::llm::RunKind;
use mimorya_core::authz::HOOK_CHANNEL;
use mimorya_db::Database;
use mimorya_db::repositories::runs::RunRow;
use mimorya_db::repositories::scheduled_tasks::started_by_a_task;

pub async fn run_kind(db: &Database, run: &RunRow) -> anyhow::Result<RunKind> {
    if run.mission_id.is_some() {
        return Ok(RunKind::Mission);
    }
    if run.channel == HOOK_CHANNEL {
        return Ok(RunKind::Hook);
    }
    let mut tx = db.begin(run.tenant_id).await?;
    let scheduled = started_by_a_task(&mut tx, run.id).await?;
    tx.commit().await?;
    Ok(match scheduled {
        true => RunKind::ScheduledTask,
        false => RunKind::Conversation,
    })
}
