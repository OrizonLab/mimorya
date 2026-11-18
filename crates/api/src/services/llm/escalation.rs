//! A company's model escalation as its runs use it (ADR-207): the stronger
//! model of its own conversations' provider, built with the same key and
//! endpoint, loaded with its other models while a mechanism the platform
//! delivers is turned on and the stored choice still serves; and what its
//! mechanisms spent this month, against which each escalation is weighed.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use mimorya_agent::llm::{
    ESCALATION_BUDGET_ROLES, EscalationChoice, EscalationRoute, EscalationRoutes, LlmProvider,
    Mechanism, TenantLlmChoice, company_provider_config, escalation_problem,
};
use mimorya_core::time_zone::month_start;
use mimorya_db::TenantTx;
use mimorya_db::repositories::llm_settings::{ModelPurpose, get_llm_settings};
use mimorya_db::repositories::model_escalation::{
    ModelEscalationRow, escalation_spent, model_escalation,
};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{LlmService, Resolution, api_key_secret};

/// The company's escalation, ready for its runs.
#[derive(Clone)]
pub struct EscalationModel {
    pub provider: Arc<dyn LlmProvider>,
    pub after_failure: bool,
    /// Whether the router is on (step 2), and its rule for each kind of run.
    pub router: bool,
    pub routes: EscalationRoutes,
    pub max_per_run: u32,
    pub monthly_budget_micros: i64,
}

/// What the escalation mechanisms spent since the start of the month.
pub struct MonthSpending {
    pub spent_micros: i64,
    pub month_start: DateTime<Utc>,
}

impl LlmService {
    /// The company's escalation, if its runs go to its own provider, a
    /// delivered mechanism is turned on and the stored choice still serves
    /// (else why not is logged, and its runs keep their main model).
    pub(super) async fn company_escalation(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
        runs: &Resolution,
    ) -> anyhow::Result<Option<EscalationModel>> {
        if !matches!(runs, Resolution::Company(_)) {
            return Ok(None);
        }
        let Some(row) = model_escalation(tx).await? else {
            return Ok(None);
        };
        let after_failure = row.after_failure && Mechanism::AfterFailure.available();
        let router = row.router && Mechanism::Router.available();
        if !after_failure && !router {
            return Ok(None);
        }
        let stored = escalation_choice(row);
        let conversations = conversations_choice(tx).await?;
        if let Some(problem) = escalation_problem(&stored, conversations.as_ref(), &self.prices) {
            tracing::warn!(%tenant_id, problem = %problem, "Company escalation unusable");
            return Ok(None);
        }
        let api_key = match self
            .secrets
            .read(tx, tenant_id, api_key_secret(ModelPurpose::Conversations))
            .await?
            .into_key()
        {
            Ok(key) => key,
            Err(problem) => {
                tracing::warn!(%tenant_id, problem = %problem, "Company escalation unusable");
                return Ok(None);
            }
        };
        let built = company_provider_config(
            &stored.provider,
            &stored.model,
            stored.base_url.as_deref(),
            api_key,
            &self.listed_endpoints,
        )
        .and_then(|config| {
            (self.factory)(&config).map_err(|_| "the provider cannot be built".to_string())
        });
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
            max_per_run: stored.max_per_run,
            monthly_budget_micros: stored.monthly_budget_micros,
        }))
    }

    /// The company's escalation, if its runs have one they can use.
    pub async fn escalation_for(&self, tenant_id: Uuid) -> anyhow::Result<Option<EscalationModel>> {
        Ok(self.resolve(tenant_id).await?.escalation.clone())
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
pub async fn conversations_choice(
    conn: &mut PgConnection,
) -> anyhow::Result<Option<TenantLlmChoice>> {
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
