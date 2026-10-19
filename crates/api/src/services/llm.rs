//! Which model provider serves a company's runs (ADR-024): its own when an
//! administrator configured one, else the platform's. When the company chose
//! its own and it cannot be used (unreadable key, refused configuration), its
//! runs stop rather than fall back: the company decided where its data goes.

use std::sync::Arc;
use std::time::Duration;

use mimorya_agent::llm::{LlmProvider, PriceTable, ProviderConfig, create_provider};
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
}

impl Resolution {
    pub fn provider(&self) -> Option<Arc<dyn LlmProvider>> {
        match self {
            Self::Platform(provider) => provider.clone(),
            Self::Company(provider) => Some(provider.clone()),
        }
    }
}

pub struct LlmService {
    db: Database,
    platform: Option<Arc<dyn LlmProvider>>,
    secrets: SecretStore,
    factory: ProviderFactory,
    prices: Arc<PriceTable>,
    cache: TenantCache<Resolution>,
}

impl LlmService {
    pub fn new(
        db: Database,
        platform: Option<Arc<dyn LlmProvider>>,
        secrets: SecretStore,
        factory: ProviderFactory,
        prices: Arc<PriceTable>,
    ) -> Self {
        Self {
            db,
            platform,
            secrets,
            factory,
            prices,
            cache: TenantCache::new(Duration::from_secs(60)),
        }
    }

    pub fn platform(&self) -> Option<&Arc<dyn LlmProvider>> {
        self.platform.as_ref()
    }

    pub fn secrets(&self) -> &SecretStore {
        &self.secrets
    }

    /// Known prices: an unpriced model is reported as such, never estimated.
    pub fn prices(&self) -> &Arc<PriceTable> {
        &self.prices
    }

    /// The company's resolution; an error when its own provider cannot be used.
    pub async fn resolve(&self, tenant_id: Uuid) -> anyhow::Result<Arc<Resolution>> {
        let generation = match self.cache.get(tenant_id) {
            Ok(resolution) => return Ok(resolution),
            Err(generation) => generation,
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let resolution = Arc::new(self.load(&mut tx, tenant_id).await?);
        tx.commit().await?;
        self.cache.store(tenant_id, generation, resolution.clone());
        Ok(resolution)
    }

    async fn load(&self, tx: &mut TenantTx, tenant_id: Uuid) -> anyhow::Result<Resolution> {
        let Some(settings) = get_llm_settings(tx).await? else {
            return Ok(Resolution::Platform(self.platform.clone()));
        };
        let api_key = self.secrets.get(tx, tenant_id, LLM_API_KEY).await?;
        let config = ProviderConfig {
            kind: settings.provider,
            model: Some(settings.model),
            api_key,
            base_url: settings.base_url,
        };
        Ok(Resolution::Company((self.factory)(&config)?))
    }

    /// The provider of the company's runs, if any can be used.
    pub async fn provider_for(
        &self,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<Arc<dyn LlmProvider>>> {
        Ok(self.resolve(tenant_id).await?.provider())
    }

    /// Whether the company's runs have a provider; a company provider that
    /// cannot be used is reported (without its configuration) and counts as none.
    pub async fn available(&self, tenant_id: Uuid) -> bool {
        match self.provider_for(tenant_id).await {
            Ok(provider) => provider.is_some(),
            Err(error) => {
                tracing::warn!(%tenant_id, error = %error, "Company model provider unusable");
                false
            }
        }
    }

    /// After a change of the company's settings.
    pub fn invalidate(&self, tenant_id: Uuid) {
        self.cache.invalidate(tenant_id);
    }
}
