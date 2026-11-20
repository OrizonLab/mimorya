//! The company's decision service (ADR-213), read and set by those who
//! manage its settings: its engine, Jev or its main model, and the mode and
//! threshold of each usage; and the engine's test.

use mimorya_agent::decision::{DecisionMode, DecisionUsage};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DecisionEngineChoice {
    /// The model of the company's runs: the sovereign mode.
    MainModel,
    /// TypeSafe's Jev, a subprocessor outside the EU.
    Jev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DecisionUsageKey {
    ModelRouting,
    SkillChoice,
    MemoryGuard,
    DocumentLabels,
    ActionGuard,
    InjectionDetection,
    MailTriage,
}

impl From<DecisionUsageKey> for DecisionUsage {
    fn from(key: DecisionUsageKey) -> Self {
        match key {
            DecisionUsageKey::ModelRouting => Self::ModelRouting,
            DecisionUsageKey::SkillChoice => Self::SkillChoice,
            DecisionUsageKey::MemoryGuard => Self::MemoryGuard,
            DecisionUsageKey::DocumentLabels => Self::DocumentLabels,
            DecisionUsageKey::ActionGuard => Self::ActionGuard,
            DecisionUsageKey::InjectionDetection => Self::InjectionDetection,
            DecisionUsageKey::MailTriage => Self::MailTriage,
        }
    }
}

impl From<DecisionUsage> for DecisionUsageKey {
    fn from(usage: DecisionUsage) -> Self {
        match usage {
            DecisionUsage::ModelRouting => Self::ModelRouting,
            DecisionUsage::SkillChoice => Self::SkillChoice,
            DecisionUsage::MemoryGuard => Self::MemoryGuard,
            DecisionUsage::DocumentLabels => Self::DocumentLabels,
            DecisionUsage::ActionGuard => Self::ActionGuard,
            DecisionUsage::InjectionDetection => Self::InjectionDetection,
            DecisionUsage::MailTriage => Self::MailTriage,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DecisionModeChoice {
    Off,
    /// Decided and journaled, without effect, to calibrate.
    Observe,
    Active,
}

impl From<DecisionModeChoice> for DecisionMode {
    fn from(mode: DecisionModeChoice) -> Self {
        match mode {
            DecisionModeChoice::Off => Self::Off,
            DecisionModeChoice::Observe => Self::Observe,
            DecisionModeChoice::Active => Self::Active,
        }
    }
}

impl From<DecisionMode> for DecisionModeChoice {
    fn from(mode: DecisionMode) -> Self {
        match mode {
            DecisionMode::Off => Self::Off,
            DecisionMode::Observe => Self::Observe,
            DecisionMode::Active => Self::Active,
        }
    }
}

/// `GET /admin/decisions`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DecisionSettingsView {
    /// The engine chosen: the main model without a choice.
    pub engine: DecisionEngineChoice,
    /// Whether the operator opened Jev, and why not otherwise (in French).
    pub jev_available: bool,
    pub jev_reason: Option<String>,
    /// The version of Jev the operator pins.
    pub jev_model: String,
    pub has_jev_key: bool,
    /// Why the engine chosen cannot decide now (in French): decisions go to
    /// the main model meanwhile.
    pub problem: Option<String>,
    /// The model of the company's runs, if it has one.
    pub main_model: Option<String>,
    pub usages: Vec<DecisionUsageView>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DecisionUsageView {
    pub key: DecisionUsageKey,
    pub label: String,
    /// Whether its step is delivered: otherwise it may only be off.
    pub available: bool,
    pub mode: DecisionModeChoice,
    pub threshold: f64,
}

/// Body of `PUT /admin/decisions`.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionSettingsUpdate {
    pub engine: DecisionEngineChoice,
    /// A new key to TypeSafe's API; the stored one kept when absent.
    #[ts(optional)]
    pub jev_api_key: Option<String>,
    pub usages: Vec<DecisionUsageUpdate>,
}

#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionUsageUpdate {
    pub key: DecisionUsageKey,
    pub mode: DecisionModeChoice,
    pub threshold: f64,
}

/// `POST /admin/decisions/test`: the engine chosen asked a fixed question.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DecisionTestResult {
    pub engine: DecisionEngineChoice,
    pub model: String,
    pub latency_ms: u64,
    /// The probability of yes to the test's question.
    pub yes: f64,
}
