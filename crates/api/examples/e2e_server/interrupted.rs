//! A run as a stop of the service leaves it (ADR-053), for the end-to-end
//! tests to see it taken up (`interrupted-run.spec.ts`): Marc's request, a
//! planning he refused to apply, the step saved, then a search of
//! interventions started whose result was lost. The server takes it up at
//! startup, as `mimorya-api` does, and the scripted model plays it again
//! from there. The refused action ended before the stop: only the search is
//! lost.

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::{DEFAULT_BUDGET, RunCheckpoint, RunEvent};
use mimorya_api::app::App;
use mimorya_api::contract::MessageMetadata;
use mimorya_core::autonomy::{AutonomyLevel, ToolRisk};
use mimorya_core::demo::{tenant, user};
use mimorya_db::TenantTx;
use mimorya_db::repositories::conversations::{NewMessage, add_message, create_conversation};
use mimorya_db::repositories::run_events::append_run_event;
use mimorya_db::repositories::runs::{NewRun, RunPatch, create_run, update_run};
use uuid::Uuid;

/// The interrupted conversation's title, which the test opens.
const TITLE: &str = "Point de la semaine interrompu";
const REQUEST: &str = "Où en sont les interventions de cette semaine ?";

/// Leaves Marc's run as the service stopped it, a search under way.
pub async fn leave(app: &App) -> anyhow::Result<()> {
    let mut tx = app.services.db.begin(tenant::ID).await?;
    let conversation = create_conversation(&mut tx, tenant::ID, user::MARC, Some(TITLE)).await?;
    add_message(
        &mut tx,
        NewMessage {
            tenant_id: tenant::ID,
            conversation_id: conversation.id,
            role: "user",
            content: REQUEST.into(),
            author_user_id: Some(user::MARC),
            run_id: None,
            metadata: serde_json::to_value(MessageMetadata::default())?,
        },
    )
    .await?;
    let run = create_run(
        &mut tx,
        NewRun {
            tenant_id: tenant::ID,
            conversation_id: Some(conversation.id),
            initiated_by_user_id: user::MARC,
            on_behalf_of_user_id: user::MARC,
            objective: REQUEST.into(),
            success_criteria: Vec::new(),
            autonomy_level: AutonomyLevel::ExecuteWithApproval,
            budget: serde_json::to_value(DEFAULT_BUDGET)?,
        },
    )
    .await?;
    let started = RunEvent::RunStarted {
        objective: REQUEST.into(),
    };
    record(&mut tx, run.id, started).await?;
    // A planning to apply, refused: the call ended, as the engine ends it.
    let (apply, approval_id) = ("refused-apply".to_string(), Uuid::new_v4());
    let events = [
        RunEvent::ToolCall {
            tool_call_id: apply.clone(),
            tool_name: "planning_apply".into(),
            title: "Application d’un planning".into(),
            decision: "require_approval".into(),
            reason: Some("Modification du planning : validation requise".into()),
            risk: Some(ToolRisk::Write),
        },
        RunEvent::ApprovalRequested {
            approval_id,
            tool_name: "planning_apply".into(),
            action_summary: "Appliquer le planning de suivi".into(),
            risk: ToolRisk::Write,
        },
        RunEvent::ApprovalResolved {
            approval_id,
            approved: false,
        },
        RunEvent::ToolResult {
            tool_call_id: apply,
            tool_name: "planning_apply".into(),
            summary: "Action refusée lors de la validation".into(),
            is_error: true,
            duration_ms: 0,
        },
    ];
    for event in events {
        record(&mut tx, run.id, event).await?;
    }
    // The step saved: its mark covers what came before.
    let saved = RunCheckpoint::new(
        "Consignes enregistrées avant l’arrêt du service.".into(),
        vec![LlmMessage::user(REQUEST)],
    );
    let patch = RunPatch {
        state: Some(serde_json::to_value(saved)?),
        ..RunPatch::default()
    };
    update_run(&mut tx, run.id, patch).await?;
    // Then a read started, as the engine records it, and lost.
    let search = RunEvent::ToolCall {
        tool_call_id: "interrupted-search".into(),
        tool_name: "interventions_search".into(),
        title: "Recherche d’interventions".into(),
        decision: "allow".into(),
        reason: None,
        risk: Some(ToolRisk::Read),
    };
    record(&mut tx, run.id, search).await?;
    tx.commit().await?;
    Ok(())
}

async fn record(tx: &mut TenantTx, run_id: Uuid, event: RunEvent) -> anyhow::Result<()> {
    let (kind, data) = event.into_parts();
    append_run_event(tx, tenant::ID, run_id, &kind, &data).await?;
    Ok(())
}
