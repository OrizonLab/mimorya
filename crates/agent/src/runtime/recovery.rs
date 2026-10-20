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

impl AgentRuntime {
    /// Goes on from `checkpoint`, the run's last saved step, once the tools
    /// named in `interrupted` (calls started since that step, in order) are
    /// known to only read. While approved actions were running (`pending`),
    /// `interrupted` names those approved, and the run ends.
    pub async fn recover(
        &self,
        mut checkpoint: RunCheckpoint,
        interrupted: Vec<String>,
    ) -> anyhow::Result<RunOutcome> {
        if let Some(error) = self.not_resumable(&checkpoint, &interrupted) {
            return self
                .conclude(RunOutcome::Failed { error, checkpoint })
                .await;
        }
        checkpoint.resumptions += 1;
        self.deps
            .ports
            .events
            .emit(RunEvent::RunResumed { interrupted })
            .await?;
        self.deps.ports.runs.checkpoint(&checkpoint).await?;
        self.run_loop(checkpoint).await
    }

    /// Why the run cannot go on, in French; `None` when it can.
    fn not_resumable(&self, checkpoint: &RunCheckpoint, interrupted: &[String]) -> Option<String> {
        if checkpoint.resumptions >= MAX_RESUMPTIONS {
            return Some(
                "Exécution interrompue à plusieurs reprises par un arrêt du service : elle \
                 n’est pas reprise une nouvelle fois. Relancez votre demande."
                    .into(),
            );
        }
        if checkpoint.pending.is_some() {
            // `interrupted` names the actions approved among those awaited.
            if interrupted.is_empty() {
                return Some(
                    "Exécution interrompue par un arrêt du service après votre décision : \
                     aucune action n’avait été validée, rien n’a été exécuté. Relancez votre \
                     demande si besoin."
                        .into(),
                );
            }
            let approved: Vec<String> = interrupted.iter().map(|n| self.title_of(n)).collect();
            return Some(format!(
                "Exécution interrompue par un arrêt du service pendant {}. Vérifiez-le avant de \
                 relancer votre demande.",
                unknown_effect(&approved, true)
            ));
        }
        let unsafe_actions: Vec<String> = interrupted
            .iter()
            .filter(|name| {
                self.tools
                    .tool(name)
                    .is_none_or(|tool| tool.info.risk != ToolRisk::Read)
            })
            .map(|name| self.title_of(name))
            .collect();
        (!unsafe_actions.is_empty()).then(|| {
            format!(
                "Exécution interrompue par un arrêt du service pendant {}. Vérifiez-le avant de \
                 relancer votre demande.",
                unknown_effect(&unsafe_actions, false)
            )
        })
    }

    /// « Title » of a tool as users know it, or its name once it is gone.
    fn title_of(&self, name: &str) -> String {
        let title = self
            .tools
            .tool(name)
            .map_or_else(|| name.to_owned(), |tool| tool.info.title);
        format!("« {title} »")
    }
}

/// « l’action « A » : son effet n’est pas connu », or the plural, of actions
/// that were `validated` or not.
fn unknown_effect(titles: &[String], validated: bool) -> String {
    let (one_kind, many_kind) = if validated {
        ("l’action validée", "les actions validées")
    } else {
        ("l’action", "les actions")
    };
    match titles {
        [one] => format!("{one_kind} {one} : son effet n’est pas connu"),
        many => format!(
            "{many_kind} {} : leur effet n’est pas connu",
            many.join(", ")
        ),
    }
}
