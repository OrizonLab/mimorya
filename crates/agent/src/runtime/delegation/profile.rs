//! The profiles a sub-agent works with (ADR-098): each says, to the child
//! and to the people reading its trace, what kind of task it was given.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentProfile {
    /// Gathers the facts and their sources on a precise question.
    Research,
    /// Examines data for findings, trends, anomalies and figures.
    Analysis,
    /// Proposes a plan or a breakdown into steps, running none of them.
    Planning,
    /// Reviews a draft against criteria and sources.
    Quality,
}

impl SubAgentProfile {
    pub const ALL: [SubAgentProfile; 4] = [
        Self::Research,
        Self::Analysis,
        Self::Planning,
        Self::Quality,
    ];

    /// As stored (`agent_runs.agent_profile`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Research => "research",
            Self::Analysis => "analysis",
            Self::Planning => "planning",
            Self::Quality => "quality",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == value)
    }

    /// As people read it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Research => "Recherche",
            Self::Analysis => "Analyse",
            Self::Planning => "Planification",
            Self::Quality => "Contrôle qualité",
        }
    }

    /// What the child is there for, as the model reads it.
    pub(super) fn guidance(self) -> &'static str {
        match self {
            Self::Research => {
                "Tu rassembles les faits utiles à la question posée : enregistrements, documents, mémoire. Chaque fait est accompagné de sa source."
            }
            Self::Analysis => {
                "Tu examines les données nécessaires pour en tirer des constats chiffrés : tendances, écarts, anomalies, en précisant sur quoi chacun repose."
            }
            Self::Planning => {
                "Tu proposes un plan ou un découpage en étapes réalisables, avec leurs dépendances et les informations qui manquent ; tu n’en exécutes aucune."
            }
            Self::Quality => {
                "Tu relis le contenu transmis au regard des critères donnés et des sources que tu peux consulter : erreurs, oublis, affirmations non étayées."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_reads_back_as_stored() {
        for profile in SubAgentProfile::ALL {
            assert_eq!(SubAgentProfile::parse(profile.as_str()), Some(profile));
            assert_eq!(
                serde_json::to_value(profile).unwrap(),
                serde_json::json!(profile.as_str())
            );
        }
        assert_eq!(SubAgentProfile::parse("main"), None);
    }
}
