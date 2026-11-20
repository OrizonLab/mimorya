//! What decisions serve, and how each usage is set (ADR-213): off by
//! default, observed (decided and journaled, without effect, to calibrate)
//! or active, with its threshold. A usage whose step is not delivered may
//! not be set: it is refused, never accepted without effect.

/// The usages, one a step of the chantier (D1 to D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecisionUsage {
    /// D1: the model a run starts on.
    ModelRouting,
    /// D2: the skills to load.
    SkillChoice,
    /// D3: a memory or a lesson holding sensitive data.
    MemoryGuard,
    /// D4: the type and confidentiality of a document deposited.
    DocumentLabels,
    /// D5: an action that calls for a human's validation.
    ActionGuard,
    /// D6: instructions for the AI in external content.
    InjectionDetection,
    /// D7: the e-mails worth waking the agent for.
    MailTriage,
}

/// The usages delivered so far.
pub const AVAILABLE_USAGES: &[DecisionUsage] = &[];

/// What the engine's test asks, journaled as such.
pub const ENGINE_TEST: &str = "engine_test";

impl DecisionUsage {
    pub const ALL: [DecisionUsage; 7] = [
        DecisionUsage::ModelRouting,
        DecisionUsage::SkillChoice,
        DecisionUsage::MemoryGuard,
        DecisionUsage::DocumentLabels,
        DecisionUsage::ActionGuard,
        DecisionUsage::InjectionDetection,
        DecisionUsage::MailTriage,
    ];

    /// As stored (`tenant_decision_usages_usage`) and journaled.
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionUsage::ModelRouting => "model_routing",
            DecisionUsage::SkillChoice => "skill_choice",
            DecisionUsage::MemoryGuard => "memory_guard",
            DecisionUsage::DocumentLabels => "document_labels",
            DecisionUsage::ActionGuard => "action_guard",
            DecisionUsage::InjectionDetection => "injection_detection",
            DecisionUsage::MailTriage => "mail_triage",
        }
    }

    pub fn parse(stored: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|usage| usage.as_str() == stored)
    }

    pub fn label(self) -> &'static str {
        match self {
            DecisionUsage::ModelRouting => "Choix du modèle d’un run",
            DecisionUsage::SkillChoice => "Choix des compétences à charger",
            DecisionUsage::MemoryGuard => "Données sensibles dans la mémoire et les leçons",
            DecisionUsage::DocumentLabels => "Type et confidentialité des documents déposés",
            DecisionUsage::ActionGuard => "Validation des actions sensibles",
            DecisionUsage::InjectionDetection => "Instructions cachées dans les contenus externes",
            DecisionUsage::MailTriage => "Tri des e-mails entrants",
        }
    }

    pub fn available(self) -> bool {
        AVAILABLE_USAGES.contains(&self)
    }
}

/// How a usage is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DecisionMode {
    #[default]
    Off,
    /// Decided and journaled, without effect.
    Observe,
    Active,
}

impl DecisionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionMode::Off => "off",
            DecisionMode::Observe => "observe",
            DecisionMode::Active => "active",
        }
    }

    pub fn parse(stored: &str) -> Option<Self> {
        [
            DecisionMode::Off,
            DecisionMode::Observe,
            DecisionMode::Active,
        ]
        .into_iter()
        .find(|mode| mode.as_str() == stored)
    }
}

/// A usage's mode and threshold, from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UsagePolicy {
    pub mode: DecisionMode,
    pub threshold: f64,
}

/// Why a usage may not be set so, in French; `None` when it may.
pub fn usage_problem(usage: DecisionUsage, policy: &UsagePolicy) -> Option<String> {
    if !usage.available() && policy.mode != DecisionMode::Off {
        return Some(format!("« {} » n’est pas encore disponible", usage.label()));
    }
    let bounded = policy.threshold.is_finite() && (0.0..=1.0).contains(&policy.threshold);
    (!bounded).then(|| format!("Le seuil de « {} » va de 0 à 1", usage.label()))
}
