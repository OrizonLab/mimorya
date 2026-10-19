//! Which model provider serves a company's runs (ADR-024): its own when an
//! administrator configured one, else the platform's. What the company stored
//! is checked again at each load against the rules as they stand (an endpoint
//! the operator removed from the list is refused). When the company's own
//! cannot be used, its runs stop rather than fall back: the company decided
//! where its data goes.

use std::sync::Arc;
use std::time::Duration;

use mimorya_agent::llm::{
    LlmProvider, PriceTable, ProviderConfig, company_provider_config, create_provider,
};
use mimorya_db::repositories::llm_settings::get_llm_settings;
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use super::tenant_cache::TenantCache;
use crate::secrets::{LLM_API_KEY, SecretStore};

/// Builds a provider from a configuration (tests substitute a scripted one).
pub type ProviderFactory =
    Arc<dyn Fn(&ProviderConfig) -> anyhow::Result<Arc<dyn LlmProvider>> + Send + Sync>;

pub fn default_factory() -> ProviderFactory {
    Arc::new(create_provider)
}

/// Where a company's runs go.
#[derive(Clone)]
pub enum Resolution {
    /// The company has no provider of its own.
    Platform(Option<Arc<dyn LlmProvider>>),
    Company(Arc<dyn LlmProvider>),
    /// The company's own cannot be used; why, in French, for its administrators.
    Refused(String),
}

/// The company chose its own provider and it cannot be used (as opposed to
/// a passing failure of the server).
#[derive(Debug)]
pub struct Unusable(pub String);

impl std::fmt::Display for Unusable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "company model provider unusable: {}", self.0)
    }
}

impl std::error::Error for Unusable {}

pub struct LlmService {
    db: Database,
    platform: Option<Arc<dyn LlmProvider>>,
    secrets: SecretStore,
    listed_endpoints: Vec<String>,
    factory: ProviderFactory,
    prices: Arc<PriceTable>,
    cache: TenantCache<Resolution>,
}

pub struct LlmServiceOptions {
    pub platform: Option<Arc<dyn LlmProvider>>,
    pub secrets: SecretStore,
    /// `MIMORYA_TENANT_LLM_ENDPOINTS`, read again at each load.
    pub listed_endpoints: Vec<String>,
    pub factory: ProviderFactory,
    pub prices: Arc<PriceTable>,
}

impl LlmService {
    pub fn new(db: Database, options: LlmServiceOptions) -> Self {
        Self {
            db,
            platform: options.platform,
            secrets: options.secrets,
            listed_endpoints: options.listed_endpoints,
            factory: options.factory,
            prices: options.prices,
            cache: TenantCache::new(Duration::from_secs(60)),
        }
    }

    pub fn platform(&self) -> Option<&Arc<dyn LlmProvider>> {
        self.platform.as_ref()
    }

    pub fn secrets(&self) -> &SecretStore {
        &self.secrets
    }

    pub fn listed_endpoints(&self) -> &[String] {
        &self.listed_endpoints
    }

    /// Known prices: an unpriced model is reported as such, never estimated.
    pub fn prices(&self) -> &Arc<PriceTable> {
        &self.prices
    }

    /// The company's resolution, a refusal included (cached like the rest);
    /// an error only when the server itself fails.
    pub async fn resolve(&self, tenant_id: Uuid) -> anyhow::Result<Arc<Resolution>> {
        let generation = match self.cache.get(tenant_id) {
            Ok(resolution) => return Ok(resolution),
            Err(generation) => generation,
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let resolution = Arc::new(self.load(&mut tx, tenant_id).await?);
        tx.commit().await?;
        if let Resolution::Refused(reason) = resolution.as_ref() {
            tracing::warn!(%tenant_id, reason = %reason, "Company model provider unusable");
        }
        self.cache.store(tenant_id, generation, resolution.clone());
        Ok(resolution)
    }

    async fn load(&self, tx: &mut TenantTx, tenant_id: Uuid) -> anyhow::Result<Resolution> {
        let Some(settings) = get_llm_settings(tx).await? else {
            return Ok(Resolution::Platform(self.platform.clone()));
        };
        let Ok(api_key) = self.secrets.get(tx, tenant_id, LLM_API_KEY).await else {
            return Ok(Resolution::Refused(
                "La clé enregistrée ne peut pas être déchiffrée (clé maîtresse du serveur \
                 changée ?) : saisissez-la de nouveau"
                    .into(),
            ));
        };
        let config = match company_provider_config(
            &settings.provider,
            &settings.model,
            settings.base_url.as_deref(),
            api_key,
            &self.listed_endpoints,
        ) {
            Ok(config) => config,
            Err(reason) => return Ok(Resolution::Refused(reason)),
        };
        Ok(match (self.factory)(&config) {
            Ok(provider) => Resolution::Company(provider),
            Err(_) => Resolution::Refused(
                "Le fournisseur ne peut pas être préparé avec cette configuration".into(),
            ),
        })
    }

    /// The provider of the company's runs, if any; `Unusable` when the
    /// company's own cannot be used.
    pub async fn provider_for(
        &self,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<Arc<dyn LlmProvider>>> {
        match self.resolve(tenant_id).await?.as_ref() {
            Resolution::Platform(provider) => Ok(provider.clone()),
            Resolution::Company(provider) => Ok(Some(provider.clone())),
            Resolution::Refused(reason) => Err(Unusable(reason.clone()).into()),
        }
    }

    /// Whether the company's runs have a provider they can use.
    pub async fn available(&self, tenant_id: Uuid) -> bool {
        match self.provider_for(tenant_id).await {
            Ok(provider) => provider.is_some(),
            Err(error) => {
                if !error.is::<Unusable>() {
                    tracing::warn!(%tenant_id, error = %error, "Model provider resolution failed");
                }
                false
            }
        }
    }

    /// After a change of the company's settings.
    pub fn invalidate(&self, tenant_id: Uuid) {
        self.cache.invalidate(tenant_id);
    }
}
