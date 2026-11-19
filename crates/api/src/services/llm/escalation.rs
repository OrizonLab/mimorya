//! A company's model escalation as its runs use it (ADR-207): the stronger
//! model of its runs' provider, built with the same key and endpoint (its
//! own provider's, or, in the formulas the operator gives it, the
//! platform's, ADR-209), loaded with its other models while a mechanism the
//! platform delivers is turned on and the stored choice still serves; and
//! what its mechanisms spent this month, against which each escalation is
//! weighed: what the company pays for them, the amount re-invoiced on the
//! operator's key. Its main model may consult it as Anthropic's advisor
//! where the operator opens it, on Anthropic at its official address.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use mimorya_agent::llm::anthropic::DEFAULT_BASE_URL;
use mimorya_agent::llm::{
    AdvisorOffer, ESCALATION_BUDGET_ROLES, EscalationChoice, EscalationRoute, EscalationRoutes,
    LlmProvider, Mechanism, ProviderConfig, TenantLlmChoice, company_provider_config,
    escalation_problem,
};
use mimorya_core::time_zone::month_start;
use mimorya_db::TenantTx;
use mimorya_db::repositories::llm_settings::{ModelPurpose, get_llm_settings};
use mimorya_db::repositories::model_escalation::{
    ModelEscalationRow, escalation_spent, model_escalation,
};
use sqlx::PgConnection;
use uuid::Uuid;

use super::supply::company_supply;
use super::{LlmService, Resolution, api_key_secret};

/// The company's escalation, ready for its runs.
#[derive(Clone)]
pub struct EscalationModel {
    pub provider: Arc<dyn LlmProvider>,
    pub after_failure: bool,
    /// Whether the router is on (step 2), and its rule for each kind of run.
    pub router: bool,
    pub routes: EscalationRoutes,
    /// The model its main model may consult as Anthropic's advisor (step
    /// 3), where the platform offers it and Anthropic accepts the pair: the
    /// escalation model, as the company chose it.
    pub advisor: Option<String>,
    pub max_per_run: u32,
    pub monthly_budget_micros: i64,
}

/// The model of a company's runs, beside which an escalation is chosen.
pub struct RunsModel {
    pub choice: TenantLlmChoice,
    /// The platform's, on the operator's key (ADR-209): what the escalation
    /// spends counts in the company's cap or credit too.
    pub on_operator_key: bool,
}

/// What the escalation mechanisms spent since the start of the month.
pub struct MonthSpending {
    pub spent_micros: i64,
    pub month_start: DateTime<Utc>,
}

impl LlmService {
    /// The model of the company's runs: its own provider's, else the
    /// platform's while the operator supplies it (ADR-209), its formula read
    /// as its runs read it; none otherwise.
    pub async fn runs_model(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<RunsModel>> {
        if let Some(choice) = conversations_choice(tx).await? {
            return Ok(Some(RunsModel {
                choice,
                on_operator_key: false,
            }));
        }
        let supplied = company_supply(tx, tenant_id)
            .await?
            .formula
            .on_operator_key();
        Ok(self
            .platform_choice()
            .filter(|_| supplied)
            .map(|choice| RunsModel {
                choice,
                on_operator_key: true,
            }))
    }

    /// Where the platform stands for the company's advisor, its runs' model
    /// being `runs` (ADR-207): opened by the operator, and on Anthropic at
    /// its official address.
    pub fn advisor_offer(&self, runs: Option<&RunsModel>) -> AdvisorOffer {
        self.advisor_offered(
            runs.is_some_and(|runs| runs.on_operator_key),
            runs.map(|runs| &runs.choice),
        )
    }

    fn advisor_offered(
        &self,
        on_operator_key: bool,
        runs: Option<&TenantLlmChoice>,
    ) -> AdvisorOffer {
        let anthropic_official = match (on_operator_key, runs) {
            (_, None) => false,
            // The operator's endpoint, as its provider is configured.
            (true, Some(_)) => self.platform_config.as_ref().is_some_and(|config| {
                config.kind == "anthropic" && at_anthropic(config.base_url.as_deref())
            }),
            // A company's Anthropic provider is always at its official address.
            (false, Some(choice)) => choice.provider == "anthropic" && choice.base_url.is_none(),
        };
        AdvisorOffer {
            operator_allows: self.advisor_allowed,
            anthropic_official,
        }
    }

    /// The platform's model as an escalation is chosen beside it: its
    /// provider, without its endpoint, which the operator alone knows.
    fn platform_choice(&self) -> Option<TenantLlmChoice> {
        let (config, provider) = (self.platform_config.as_ref()?, self.platform.as_ref()?);
        Some(TenantLlmChoice {
            provider: config.kind.clone(),
            model: provider.model().to_string(),
            base_url: None,
        })
    }

    /// The company's escalation, if its runs have a model (its own
    /// provider's or the platform's, ADR-209), a delivered mechanism is
    /// turned on and the stored choice still serves (else why not is
    /// logged, and its runs keep their main model).
    pub(super) async fn company_escalation(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
        runs: &Resolution,
    ) -> anyhow::Result<Option<EscalationModel>> {
        // The platform's model serves only a company the operator supplies.
        let on_operator_key = match runs {
            Resolution::Company(_) => false,
            Resolution::Platform(Some(_)) => true,
            Resolution::Platform(None) | Resolution::Refused(_) => return Ok(None),
        };
        let Some(row) = model_escalation(tx).await? else {
            return Ok(None);
        };
        let after_failure = row.after_failure && Mechanism::AfterFailure.available();
        let router = row.router && Mechanism::Router.available();
        let advisor = row.advisor && Mechanism::Advisor.available();
        if !after_failure && !router && !advisor {
            return Ok(None);
        }
        let stored = escalation_choice(row);
        let runs_choice = match on_operator_key {
            true => self.platform_choice(),
            false => conversations_choice(tx).await?,
        };
        if let Some(problem) = escalation_problem(&stored, runs_choice.as_ref(), &self.prices) {
            tracing::warn!(%tenant_id, problem = %problem, "Company escalation unusable");
            return Ok(None);
        }
        // Where the platform no longer offers it, or Anthropic no longer
        // accepts the pair, the other mechanisms go on without it.
        let main_model = runs_choice
            .as_ref()
            .map_or("", |choice| choice.model.as_str());
        let offer = self.advisor_offered(on_operator_key, runs_choice.as_ref());
        let advisor = advisor
            && match offer.pair_problem(main_model, &stored.model) {
                None => true,
                Some(problem) => {
                    tracing::warn!(%tenant_id, problem = %problem, "Company advisor unusable");
                    false
                }
            };
        if !after_failure && !router && !advisor {
            return Ok(None);
        }
        let built = match on_operator_key {
            true => self.platform_escalation(&stored.model),
            false => self.own_escalation(tx, tenant_id, &stored).await?,
        };
        let provider = match built {
            Ok(provider) => provider,
            Err(problem) => {
                tracing::warn!(%tenant_id, problem = %problem, "Company escalation unusable");
                return Ok(None);
            }
        };
        Ok(Some(EscalationModel {
            provider,
            after_failure,
            router,
            routes: stored.routes,
            advisor: advisor.then(|| stored.model.clone()),
            max_per_run: stored.max_per_run,
            monthly_budget_micros: stored.monthly_budget_micros,
        }))
    }

    /// The escalation model on the operator's key: the platform's provider,
    /// its key and endpoint, with the model chosen (ADR-209). The company's
    /// own key is never given to it.
    fn platform_escalation(&self, model: &str) -> Result<Arc<dyn LlmProvider>, String> {
        let config = self
            .platform_config
            .as_ref()
            .ok_or_else(|| "the platform's provider is not configured".to_string())?;
        let config = ProviderConfig {
            model: Some(model.to_string()),
            ..config.clone()
        };
        (self.factory)(&config).map_err(|_| "the provider cannot be built".to_string())
    }

    /// The escalation model on the company's own key and endpoint.
    async fn own_escalation(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
        stored: &EscalationChoice,
    ) -> anyhow::Result<Result<Arc<dyn LlmProvider>, String>> {
        let api_key = match self
            .secrets
            .read(tx, tenant_id, api_key_secret(ModelPurpose::Conversations))
            .await?
            .into_key()
        {
            Ok(key) => key,
            Err(problem) => return Ok(Err(problem.to_string())),
        };
        Ok(company_provider_config(
            &stored.provider,
            &stored.model,
            stored.base_url.as_deref(),
            api_key,
            &self.listed_endpoints,
        )
        .and_then(|config| {
            (self.factory)(&config).map_err(|_| "the provider cannot be built".to_string())
        }))
    }
}

/// What the company's escalation mechanisms spent this month.
pub async fn month_spending(conn: &mut PgConnection) -> anyhow::Result<MonthSpending> {
    let month_start = month_start(Utc::now());
    let roles = ESCALATION_BUDGET_ROLES.map(|role| role.as_str());
    Ok(MonthSpending {
        spent_micros: escalation_spent(conn, month_start, &roles).await?,
        month_start,
    })
}

/// The model of the company's conversations, if it has its own provider.
async fn conversations_choice(conn: &mut PgConnection) -> anyhow::Result<Option<TenantLlmChoice>> {
    Ok(get_llm_settings(conn, ModelPurpose::Conversations)
        .await?
        .map(|row| TenantLlmChoice {
            provider: row.provider,
            model: row.model,
            base_url: row.base_url,
        }))
}

/// The choice a stored row holds.
pub fn escalation_choice(row: ModelEscalationRow) -> EscalationChoice {
    EscalationChoice {
        provider: row.provider,
        base_url: row.base_url,
        model: row.model,
        after_failure: row.after_failure,
        router: row.router,
        advisor: row.advisor,
        // Kept to the routes' names by the table.
        routes: EscalationRoutes {
            conversation: stored_route(&row.route_conversation),
            mission: stored_route(&row.route_mission),
            scheduled_task: stored_route(&row.route_scheduled_task),
            hook: stored_route(&row.route_hook),
        },
        monthly_budget_micros: row.monthly_budget_micros,
        // Kept between 1 and 50 by the table.
        max_per_run: u32::try_from(row.max_per_run).unwrap_or(1),
    }
}

fn stored_route(stored: &str) -> EscalationRoute {
    EscalationRoute::parse(stored).unwrap_or_default()
}

/// Whether an endpoint is Anthropic's official address (none: the
/// provider's default, which is it).
fn at_anthropic(base_url: Option<&str>) -> bool {
    base_url.is_none_or(|url| url.trim_end_matches('/') == DEFAULT_BASE_URL)
}
