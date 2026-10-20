//! A run taken up again after the service stopped (ADR-053): it goes on from
//! its last saved step, which is all a model call and tool calls since then
//! lost. That is safe only when nothing but reading was under way: an action
//! that may have changed something, or have started to, is never repeated
//! in the dark. The run then ends, saying which action to check. Neither is
//! a run taken up while approved actions were running (their outcome is not
//! known), nor one the service already stopped too many times.

use mimorya_core::autonomy::ToolRisk;

use super::ports::RunEvent;
use super::runtime::AgentRuntime;
use super::state::RunCheckpoint;
use super::types::RunOutcome;

/// Times a run is taken up again; a run that stops the service each time it
/// goes on is not taken up forever.
pub const MAX_RESUMPTIONS: u32 = 2;

/// A tool call a stop of the service may have left half done.
#[derive(Debug, Clone, PartialEq)]
pub struct InterruptedCall {
    pub tool_name: String,
    /// As users know it (the tool's title, or the action validated).
    pub title: String,
    /// The tool's risk when it was called; `None` when not recorded.
    pub risk: Option<ToolRisk>,
}

impl InterruptedCall {
    /// Whether it only read: as recorded when it was called and as its tool
    /// now stands (`current`, when known), the higher risk of the two
    /// deciding, so that a tool found since to write is not taken for a
    /// read. A call known by neither did not only read.
    pub fn only_read(&self, current: Option<ToolRisk>) -> bool {
        self.risk.max(current) == Some(ToolRisk::Read)
    }
}

/// « l’action « A » : son effet n’est pas connu », or the plural, of actions
/// that were `validated` or not.
pub fn unknown_effect(calls: &[InterruptedCall], validated: bool) -> String {
    let (one_kind, many_kind) = if validated {
        ("l’action validée", "les actions validées")
    } else {
        ("l’action", "les actions")
    };
    let titles: Vec<String> = calls.iter().map(|c| format!("« {} »", c.title)).collect();
    match titles.as_slice() {
        [one] => format!("{one_kind} {one} : son effet n’est pas connu"),
        many => format!(
            "{many_kind} {} : leur effet n’est pas connu",
            many.join(", ")
        ),
    }
}

impl AgentRuntime {
    /// Goes on from `checkpoint`, the run's last saved step, once the calls
    /// `interrupted` (started since that step, in order) are known to only
    /// read. While approved actions were running (`pending`), `interrupted`
    /// holds those approved, and the run ends.
    pub async fn recover(
        &self,
        mut checkpoint: RunCheckpoint,
        interrupted: Vec<InterruptedCall>,
    ) -> anyhow::Result<RunOutcome> {
        if let Some(error) = self.not_resumable(&checkpoint, &interrupted) {
            return self
                .conclude(RunOutcome::Failed { error, checkpoint })
                .await;
        }
        checkpoint.resumptions += 1;
        let interrupted = interrupted.into_iter().map(|c| c.tool_name).collect();
        self.deps
            .ports
            .events
            .emit(RunEvent::RunResumed { interrupted })
            .await?;
        self.deps.ports.runs.checkpoint(&checkpoint).await?;
        self.run_loop(checkpoint).await
    }

    /// Why the run cannot go on, in French; `None` when it can.
    fn not_resumable(
        &self,
        checkpoint: &RunCheckpoint,
        interrupted: &[InterruptedCall],
    ) -> Option<String> {
        if checkpoint.resumptions >= MAX_RESUMPTIONS {
            return Some(
                "Exécution interrompue à plusieurs reprises par un arrêt du service : elle \
                 n’est pas reprise une nouvelle fois. Relancez votre demande."
                    .into(),
            );
        }
        if checkpoint.pending.is_some() {
            if interrupted.is_empty() {
                return Some(
                    "Exécution interrompue par un arrêt du service après votre décision : \
                     aucune action n’avait été validée, rien n’a été exécuté. Relancez votre \
                     demande si besoin."
                        .into(),
                );
            }
            return Some(format!(
                "Exécution interrompue par un arrêt du service pendant {}. Vérifiez-le avant de \
                 relancer votre demande.",
                unknown_effect(interrupted, true)
            ));
        }
        let unknown: Vec<InterruptedCall> = interrupted
            .iter()
            .filter(|call| !call.only_read(self.current_risk(&call.tool_name)))
            .cloned()
            .collect();
        (!unknown.is_empty()).then(|| {
            format!(
                "Exécution interrompue par un arrêt du service pendant {}. Vérifiez-le avant de \
                 relancer votre demande.",
                unknown_effect(&unknown, false)
            )
        })
    }

    /// The risk of a tool still present, as the run would apply it now.
    fn current_risk(&self, tool_name: &str) -> Option<ToolRisk> {
        self.tools.tool(tool_name).map(|tool| tool.info.risk)
    }
}
