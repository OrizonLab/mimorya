//! A company's decision service (ADR-213), built for each use with its
//! engines in turn: Jev first when it chose it, the operator allows it and
//! its key opens; then its main model, on its key or the operator's, whose
//! supply is asked before each call (ADR-208). Each call that answered is
//! recorded with the role « décision »: Jev's on the company's own key, the
//! main model's as its other calls are charged.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mimorya_agent::decision::{
    DecisionCall, DecisionMode, DecisionRecorder, DecisionService, DecisionUsage, EngineKind,
    JEV_ENDPOINT, JevClient, JevEngine, MainModelEngine, RunDecisions, UsagePolicy,
};
use mimorya_agent::llm::ModelRole;
use mimorya_db::Database;
use mimorya_db::repositories::decision_settings::{decision_settings, decision_usages};
use mimorya_db::repositories::usage::{NewUsage, UsageCharge, append_usage};
use uuid::Uuid;

use super::limits::SupplyGuard;
use super::{LlmService, Models};
use crate::config::AppConfig;
use crate::secrets::TYPESAFE_API_KEY;

/// How long the main model may take to decide.
const MAIN_MODEL_TIMEOUT: Duration = Duration::from_secs(30);

/// What the operator set of decisions.
#[derive(Debug, Clone)]
pub struct DecisionConfig {
    pub jev_allowed: bool,
    pub jev_model: String,
    /// TypeSafe's API, a local one in the tests.
    pub jev_api: String,
    pub jev_timeout: Duration,
    /// Shared by every Jev engine, its connections kept.
    pub jev_http: JevClient,
}

impl DecisionConfig {
    pub fn new(config: &AppConfig, jev_api: Option<String>) -> Self {
        Self {
            jev_allowed: config.typesafe_jev,
            jev_model: config.jev_model.clone(),
            jev_api: jev_api.unwrap_or_else(|| JEV_ENDPOINT.to_string()),
            jev_timeout: config.decision_timeout,
            jev_http: JevClient::new(),
        }
    }
}

/// Whose decisions they are: the run they serve, if any, and the person.
#[derive(Debug, Clone, Copy, Default)]
pub struct DecisionScope {
    pub run_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

/// The engine a company chose, Jev only where it may be asked.
enum Chosen {
    MainModel,
    /// With its key, or why it cannot be used (in French).
    Jev(Result<String, String>),
}

impl LlmService {
    pub fn decision_config(&self) -> &DecisionConfig {
        &self.decisions
    }

    /// The company's decision service: its engines in turn.
    pub async fn decision_service(
        &self,
        tenant_id: Uuid,
        scope: DecisionScope,
    ) -> anyhow::Result<DecisionService> {
        let models = self.resolve(tenant_id).await?;
        self.decision_service_on(tenant_id, scope, &models).await
    }

    /// The same, on `models` taken already (a run's execution, ADR-208).
    pub async fn decision_service_on(
        &self,
        tenant_id: Uuid,
        scope: DecisionScope,
        models: &Arc<Models>,
    ) -> anyhow::Result<DecisionService> {
        let mut service = self.decisions_of(tenant_id, scope, models);
        if let Chosen::Jev(Ok(key)) = self.chosen(tenant_id).await? {
            service = service.then(self.jev(key), self.decisions.jev_timeout);
        }
        if let Some(main_model) = self.main_model(tenant_id, models) {
            service = service.then(main_model, MAIN_MODEL_TIMEOUT);
        }
        Ok(service)
    }

    /// The decisions a run of the company may ask, on its execution's
    /// `models` — of `only` this usage, when one is named: none while no
    /// usage wanted is set, so that no engine is built nor key read for
    /// nothing.
    pub async fn run_decisions(
        &self,
        tenant_id: Uuid,
        scope: DecisionScope,
        models: &Arc<Models>,
        only: Option<DecisionUsage>,
    ) -> anyhow::Result<Option<Arc<RunDecisions>>> {
        let mut tx = self.db.begin(tenant_id).await?;
        let rows = decision_usages(&mut tx).await?;
        tx.commit().await?;
        let policies: Vec<(DecisionUsage, UsagePolicy)> = rows
            .iter()
            .filter_map(|row| {
                let usage = DecisionUsage::parse(&row.usage)?;
                let mode = DecisionMode::parse(&row.mode)?;
                let wanted = only.is_none_or(|only| only == usage);
                let set = wanted && usage.available() && mode != DecisionMode::Off;
                set.then_some((
                    usage,
                    UsagePolicy {
                        mode,
                        threshold: row.threshold,
                    },
                ))
            })
            .collect();
        if policies.is_empty() {
            return Ok(None);
        }
        let service = self.decision_service_on(tenant_id, scope, models).await?;
        Ok(Some(Arc::new(RunDecisions::new(
            Arc::new(service),
            policies,
        ))))
    }

    /// The engine the company chose, alone: what its test asks, or why it
    /// cannot be asked (in French).
    pub async fn chosen_decision_engine(
        &self,
        tenant_id: Uuid,
        scope: DecisionScope,
    ) -> anyhow::Result<Result<DecisionService, String>> {
        let models = self.resolve(tenant_id).await?;
        let service = self.decisions_of(tenant_id, scope, &models);
        Ok(match self.chosen(tenant_id).await? {
            Chosen::Jev(Ok(key)) => Ok(service.then(self.jev(key), self.decisions.jev_timeout)),
            Chosen::Jev(Err(reason)) => Err(reason),
            Chosen::MainModel => match self.main_model(tenant_id, &models) {
                Some(engine) => Ok(service.then(engine, MAIN_MODEL_TIMEOUT)),
                None => Err(NO_MAIN_MODEL.into()),
            },
        })
    }

    /// What an administrator reads of the engines: the model of the
    /// company's runs, if any, and why the engine chosen cannot decide now
    /// (in French).
    pub async fn decision_engines(&self, tenant_id: Uuid) -> anyhow::Result<DecisionEngines> {
        let models = self.resolve(tenant_id).await?;
        let main_model = models
            .runs_provider()
            .ok()
            .flatten()
            .map(|provider| provider.model().to_string());
        let problem = match self.chosen(tenant_id).await? {
            Chosen::Jev(Ok(_)) => None,
            Chosen::Jev(Err(reason)) => Some(reason),
            Chosen::MainModel => main_model.is_none().then(|| NO_MAIN_MODEL.to_string()),
        };
        Ok(DecisionEngines {
            main_model,
            problem,
        })
    }

    fn decisions_of(
        &self,
        tenant_id: Uuid,
        scope: DecisionScope,
        models: &Arc<Models>,
    ) -> DecisionService {
        let recorder = Arc::new(DbDecisionRecorder {
            db: self.db.clone(),
            tenant_id,
            scope,
            models: models.clone(),
        });
        DecisionService::new(recorder, self.prices.clone())
    }

    async fn chosen(&self, tenant_id: Uuid) -> anyhow::Result<Chosen> {
        let mut tx = self.db.begin(tenant_id).await?;
        let engine = decision_settings(&mut tx).await?.map(|row| row.engine);
        if engine.as_deref() != Some("jev") {
            tx.commit().await?;
            return Ok(Chosen::MainModel);
        }
        if !self.decisions.jev_allowed {
            tx.commit().await?;
            return Ok(Chosen::Jev(Err(JEV_CLOSED.into())));
        }
        let key = self
            .secrets
            .read(&mut tx, tenant_id, TYPESAFE_API_KEY)
            .await?;
        tx.commit().await?;
        Ok(Chosen::Jev(match key.into_key() {
            Ok(Some(key)) => Ok(key),
            Ok(None) => Err("Aucune clé d’API TypeSafe n’est enregistrée".into()),
            Err(reason) => Err(reason),
        }))
    }

    fn jev(&self, key: String) -> Arc<JevEngine> {
        let config = &self.decisions;
        Arc::new(JevEngine::new(
            config.jev_http.clone(),
            key,
            &config.jev_model,
            &config.jev_api,
        ))
    }

    fn main_model(&self, tenant_id: Uuid, models: &Arc<Models>) -> Option<Arc<MainModelEngine>> {
        let provider = models.runs_provider().ok().flatten()?;
        let supply = Arc::new(SupplyGuard::new(self.db.clone(), tenant_id, models.clone()));
        Some(Arc::new(MainModelEngine::new(provider, Some(supply))))
    }
}

/// What an administrator reads of the engines.
#[derive(Debug, Clone)]
pub struct DecisionEngines {
    pub main_model: Option<String>,
    pub problem: Option<String>,
}

const NO_MAIN_MODEL: &str = "Votre entreprise n’a pas de modèle pour ses conversations";

/// Why Jev may not be asked, whatever the company chose.
pub const JEV_CLOSED: &str = "L’exploitant de la plateforme n’a pas ouvert Jev : vos décisions \
     sont prises par votre modèle principal";

/// Each call recorded with the role « décision ».
struct DbDecisionRecorder {
    db: Database,
    tenant_id: Uuid,
    scope: DecisionScope,
    models: Arc<Models>,
}

#[async_trait]
impl DecisionRecorder for DbDecisionRecorder {
    async fn record(&self, call: DecisionCall) -> anyhow::Result<i64> {
        let cost_micros = call.cost_micros.unwrap_or(0);
        let charge = match call.engine {
            // On the company's own key to TypeSafe.
            EngineKind::Jev => UsageCharge::CLIENT_KEY,
            EngineKind::MainModel => self.models.charge(ModelRole::Decision, cost_micros),
        };
        let mut tx = self.db.begin(self.tenant_id).await?;
        append_usage(
            &mut tx,
            NewUsage {
                tenant_id: self.tenant_id,
                user_id: self.scope.user_id,
                run_id: self.scope.run_id,
                kind: "llm",
                role: ModelRole::Decision.as_str(),
                provider: call.provider,
                model: call.model,
                input_tokens: i32::try_from(call.input_tokens).unwrap_or(i32::MAX),
                output_tokens: i32::try_from(call.output_tokens).unwrap_or(i32::MAX),
                cost_micros,
                charge,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(charge.charged(cost_micros))
    }
}
