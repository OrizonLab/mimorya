//! What a company may choose as its model escalation (ADR-207): a model of
//! the same provider as its runs', with the same key and endpoint, called
//! when its mechanisms ask for it, within a monthly budget (USD, as every
//! cost of the platform) and a number of escalations per run. Its runs'
//! model is its own provider's, which it pays for, or the platform's in the
//! formulas the operator gives it, whose cap or credit bounds it as well
//! (ADR-209); a company without either may choose none. The model has a
//! price the platform knows: unpriced, its calls would count for nothing,
//! and the budget would never bound them.

mod routes;

pub use routes::{EscalationRoute, EscalationRoutes, RunKind};

use super::advisor::AdvisorOffer;
use super::pricing::{PriceTable, model_alias};
use super::router::ModelRole;
use super::settings::{TenantLlmChoice, check_model_name};

/// A monthly budget from 1 to 10,000 USD.
pub const MIN_ESCALATION_BUDGET_MICROS: i64 = 1_000_000;
pub const MAX_ESCALATION_BUDGET_MICROS: i64 = 10_000_000_000;
/// From 1 to 50 calls to the escalation model per run.
pub const MAX_ESCALATIONS_PER_RUN: u32 = 50;

/// The three mechanisms, independent of each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mechanism {
    /// After an answer fails its review (step 1).
    AfterFailure,
    /// The model's level chosen at the start of a run (step 2).
    Router,
    /// Anthropic's advisor, consulted by the model (step 3).
    Advisor,
}

/// The mechanisms delivered: one not delivered would be refused, never
/// accepted without effect.
pub const AVAILABLE_MECHANISMS: &[Mechanism] = &[
    Mechanism::AfterFailure,
    Mechanism::Router,
    Mechanism::Advisor,
];

impl Mechanism {
    pub fn label(self) -> &'static str {
        match self {
            Mechanism::AfterFailure => "Escalade après échec du contrôle",
            Mechanism::Router => "Routeur en amont",
            Mechanism::Advisor => "Conseiller d’Anthropic (advisor)",
        }
    }

    pub fn available(self) -> bool {
        AVAILABLE_MECHANISMS.contains(&self)
    }
}

/// What an administrator asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscalationRequest<'a> {
    pub model: &'a str,
    pub after_failure: bool,
    pub router: bool,
    pub advisor: bool,
    /// The router's rule for each kind of run (step 2).
    pub routes: EscalationRoutes,
    pub monthly_budget_micros: i64,
    pub max_per_run: u32,
}

/// A choice, once checked: the provider and endpoint of the runs' model it
/// was made with are kept beside it (none for the platform's, ADR-209).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscalationChoice {
    pub provider: String,
    pub base_url: Option<String>,
    pub model: String,
    pub after_failure: bool,
    pub router: bool,
    pub advisor: bool,
    pub routes: EscalationRoutes,
    pub monthly_budget_micros: i64,
    pub max_per_run: u32,
}

impl EscalationRequest<'_> {
    fn mechanisms(&self) -> impl Iterator<Item = Mechanism> {
        [
            (Mechanism::AfterFailure, self.after_failure),
            (Mechanism::Router, self.router),
            (Mechanism::Advisor, self.advisor),
        ]
        .into_iter()
        .filter_map(|(mechanism, on)| on.then_some(mechanism))
    }
}

/// The request checked against the model of the company's runs (its own
/// provider's, or the platform's, ADR-209), or why it is refused (in
/// French): the advisor, where the platform offers it and Anthropic accepts
/// the pair.
pub fn check_escalation(
    conversations: Option<&TenantLlmChoice>,
    request: &EscalationRequest<'_>,
    prices: &PriceTable,
    advisor: AdvisorOffer,
) -> Result<EscalationChoice, String> {
    let conversations = conversations.ok_or_else(|| NO_OWN_PROVIDER.to_string())?;
    let model = request.model.trim();
    if model.is_empty() {
        return Err("Indiquez le modèle d’escalade".into());
    }
    check_model_name(model)?;
    if model_alias(model) == model_alias(&conversations.model) {
        return Err(same_model(&conversations.model));
    }
    if !prices.has(model) {
        return Err(format!(
            "La plateforme ne connaît pas le prix de « {model} » : sans prix, le budget \
             d’escalade ne pourrait pas borner ses appels"
        ));
    }
    if !(MIN_ESCALATION_BUDGET_MICROS..=MAX_ESCALATION_BUDGET_MICROS)
        .contains(&request.monthly_budget_micros)
    {
        return Err("Le budget mensuel d’escalade va de 1 à 10 000 USD".into());
    }
    if !(1..=MAX_ESCALATIONS_PER_RUN).contains(&request.max_per_run) {
        return Err(format!(
            "Le nombre d’escalades par run va de 1 à {MAX_ESCALATIONS_PER_RUN}"
        ));
    }
    if let Some(mechanism) = request.mechanisms().find(|m| !m.available()) {
        return Err(format!(
            "« {} » n’est pas encore disponible",
            mechanism.label()
        ));
    }
    if request.advisor
        && let Some(problem) = advisor.pair_problem(&conversations.model, model)
    {
        return Err(problem);
    }
    Ok(EscalationChoice {
        provider: conversations.provider.clone(),
        base_url: conversations.base_url.clone(),
        model: model.to_string(),
        after_failure: request.after_failure,
        router: request.router,
        advisor: request.advisor,
        routes: request.routes,
        monthly_budget_micros: request.monthly_budget_micros,
        max_per_run: request.max_per_run,
    })
}

/// Why a stored choice no longer serves, the model of the company's runs
/// and the platform's prices being what they are now; `None` while it
/// does. Runs then keep the main model, and the screen says why (in
/// French).
pub fn escalation_problem(
    stored: &EscalationChoice,
    conversations: Option<&TenantLlmChoice>,
    prices: &PriceTable,
) -> Option<String> {
    let Some(conversations) = conversations else {
        return Some(NO_OWN_PROVIDER.to_string());
    };
    if conversations.provider != stored.provider || conversations.base_url != stored.base_url {
        return Some(
            "Le fournisseur ou l’adresse des conversations a changé depuis ce réglage : \
             enregistrez de nouveau le modèle d’escalade"
                .into(),
        );
    }
    if model_alias(&conversations.model) == model_alias(&stored.model) {
        return Some(same_model(&conversations.model));
    }
    (!prices.has(&stored.model)).then(|| {
        format!(
            "La plateforme ne connaît plus le prix de « {} » : le budget d’escalade ne \
             pourrait pas borner ses appels",
            stored.model
        )
    })
}

/// The roles of the model calls the escalation budget counts: what the
/// mechanisms add to the main model's calls (not the reviews of answers).
pub const ESCALATION_BUDGET_ROLES: [ModelRole; 3] = [
    ModelRole::Escalation,
    ModelRole::Classification,
    ModelRole::Advisor,
];

const NO_OWN_PROVIDER: &str = "L’escalade demande un modèle pour les conversations de \
     l’entreprise : son fournisseur propre, ou le modèle de la plateforme que l’exploitant lui \
     fournit";

fn same_model(model: &str) -> String {
    format!("Le modèle d’escalade doit différer du modèle principal (« {model} »)")
}

#[cfg(test)]
#[path = "escalation/tests.rs"]
mod tests;
