//! The end of a mission's stage (ADR-089): its budget spent, the model is
//! asked for a progress note, calling no tool, and the run ends with it
//! instead of failing. The next stage starts from the mission's
//! conversation, where the note is kept, with the sources found so far.
//! The note costs one model call beyond the stage's budget, no more; a tool
//! the model still calls is never run.

use super::model_call::{ModelTurn, call_model};
use super::ports::{RunEvent, StepAccess};
use super::runtime::AgentRuntime;
use super::state::RunCheckpoint;
use super::types::RunOutcome;
use crate::llm::{LlmMessage, StopReason};

/// The note of a stage whose model wrote none: the next stage starts from
/// the mission and the earlier notes alone.
pub const NO_NOTE: &str = "Point d’étape non rédigé : l’étape suivante repart de la mission et \
     des points d’étape précédents.";

/// What the model is asked once its stage's budget is spent (`reason`).
fn note_request(reason: &str) -> String {
    format!(
        "[Fin de l’étape — message de la plateforme]\nLe budget de cette étape de la mission est \
         atteint ({reason}). N’appelle plus aucun outil : rédige ton point d’étape. L’étape \
         suivante ne verra que la mission et les points d’étape, pas les résultats des outils : \
         indique ce qui est fait (avec les identifiants et les sources utiles), ce qui reste à \
         faire et la prochaine action."
    )
}

impl AgentRuntime {
    /// Ends the stage with the progress note the model writes, its budget
    /// spent (`reason`).
    pub(super) async fn hand_over(
        &self,
        mut state: RunCheckpoint,
        reason: &str,
    ) -> anyhow::Result<RunOutcome> {
        let access = match self.deps.access.current().await? {
            StepAccess::Granted(access) => access,
            StepAccess::Ended(reason) => return self.end_without_access(state, reason).await,
        };
        state.messages.push(LlmMessage::user(note_request(reason)));
        // The tools stay declared: the conversation holds their calls, and
        // the request its cached prefix.
        let response = match call_model(&self.deps, &self.config, &mut state, &access).await? {
            ModelTurn::Response(response) => response,
            ModelTurn::Failed(error) => {
                let checkpoint = state;
                return self
                    .conclude(RunOutcome::Failed { error, checkpoint })
                    .await;
            }
        };
        if response.stop_reason == StopReason::Refusal {
            let error = "Le modèle a refusé de rédiger le point d’étape.".to_string();
            let checkpoint = state;
            return self
                .conclude(RunOutcome::Failed { error, checkpoint })
                .await;
        }
        // Its text alone: a call it still made is dropped, never run.
        state
            .messages
            .push(LlmMessage::assistant(response.text.clone()));
        let note = match response.text.trim() {
            "" => NO_NOTE.to_string(),
            text => text.to_string(),
        };
        self.deps
            .ports
            .events
            .emit(RunEvent::AssistantMessage {
                text: note.clone(),
                is_final: true,
            })
            .await?;
        let checkpoint = state;
        self.conclude(RunOutcome::Handoff { note, checkpoint })
            .await
    }
}
