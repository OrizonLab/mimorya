//! A run as a stop of the service leaves it (ADR-053), for the end-to-end
//! tests to see it taken up (`interrupted-run.spec.ts`): Marc's request,
//! saved before its first model call, then a search of interventions started
//! whose result was lost. The server takes it up at startup, as
//! `mimorya-api` does, and the scripted model plays it again from there.

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::{DEFAULT_BUDGET, RunCheckpoint, RunEvent};
use mimorya_api::app::App;
use mimorya_api::contract::MessageMetadata;
use mimorya_core::autonomy::{AutonomyLevel, ToolRisk};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::conversations::{NewMessage, add_message, create_conversation};
use mimorya_db::repositories::run_events::append_run_event;
use mimorya_db::repositories::runs::{NewRun, RunPatch, create_run, update_run};

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
    let (kind, data) = started.into_parts();
    append_run_event(&mut tx, tenant::ID, run.id, &kind, &data).await?;
    // Saved before its first model call: its mark covers the start.
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
    let (kind, data) = search.into_parts();
    append_run_event(&mut tx, tenant::ID, run.id, &kind, &data).await?;
    tx.commit().await?;
    Ok(())
}
