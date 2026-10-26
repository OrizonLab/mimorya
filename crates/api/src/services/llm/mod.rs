//! Which model providers serve a company (ADR-024): for its runs, its own
//! when an administrator configured one, else the platform's; for reviewing
//! their answers, one it chose apart, else the same as its runs (ADR-108).
//! When the company's own provider for its runs cannot be used, its runs
//! stop rather than fall back: the company decided where its data goes. When
//! the one it chose for reviews cannot be used, its runs' model reviews
//! meanwhile: it already reads all that a review reads.

use std::sync::Arc;
use std::time::Duration;

use mimorya_agent::llm::{LlmProvider, PriceTable, ProviderConfig, create_provider};
use mimorya_db::repositories::llm_settings::{ModelPurpose, get_llm_settings};
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use super::invalidations::{Invalidation, Invalidations};
use super::tenant_cache::TenantCache;
use crate::secrets::SecretStore;

mod load;

pub use load::api_key_secret;

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

/// Where a company's answers are reviewed (ADR-108).
#[derive(Clone)]
pub enum Review {
    /// By the model of its runs: it chose none apart.
    Runs,
    Company(Arc<dyn LlmProvider>),
    /// The one it chose cannot be used; why, in French, for its
    /// administrators. The model of its runs reviews meanwhile.
    Refused(String),
}

/// A company's models, loaded together.
pub struct Models {
    pub runs: Resolution,
    pub review: Review,
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
    cache: TenantCache<Models>,
    invalidations: Invalidations,
}

pub struct LlmServiceOptions {
    pub platform: Option<Arc<dyn LlmProvider>>,
    pub secrets: SecretStore,
    /// `MIMORYA_TENANT_LLM_ENDPOINTS` (read at startup), checked again at
    /// each load.
    pub listed_endpoints: Vec<String>,
    pub factory: ProviderFactory,
    pub prices: Arc<PriceTable>,
    pub invalidations: Invalidations,
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
            invalidations: options.invalidations,
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

    /// The company's models, a refusal included (cached like the rest); an
    /// error only when the server itself fails.
    pub async fn resolve(&self, tenant_id: Uuid) -> anyhow::Result<Arc<Models>> {
        let generation = match self.cache.get(tenant_id) {
            Ok(models) => return Ok(models),
            Err(generation) => generation,
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let models = Arc::new(Models {
            runs: self.load_runs(&mut tx, tenant_id).await?,
            review: self.load_review(&mut tx, tenant_id).await?,
        });
        tx.commit().await?;
        if let Resolution::Refused(reason) = &models.runs {
            tracing::warn!(%tenant_id, reason = %reason, "Company model provider unusable");
        }
        if let Review::Refused(reason) = &models.review {
            tracing::warn!(%tenant_id, reason = %reason, "Company review model unusable");
        }
        self.cache.store(tenant_id, generation, models.clone());
        Ok(models)
    }

    async fn load_runs(&self, tx: &mut TenantTx, tenant_id: Uuid) -> anyhow::Result<Resolution> {
        let purpose = ModelPurpose::Conversations;
        let Some(settings) = get_llm_settings(tx, purpose).await? else {
            return Ok(Resolution::Platform(self.platform.clone()));
        };
        Ok(
            match self
                .company_provider(tx, tenant_id, purpose, &settings)
                .await?
            {
                Ok(provider) => Resolution::Company(provider),
                Err(reason) => Resolution::Refused(reason),
            },
        )
    }

    async fn load_review(&self, tx: &mut TenantTx, tenant_id: Uuid) -> anyhow::Result<Review> {
        let purpose = ModelPurpose::Verification;
        let Some(settings) = get_llm_settings(tx, purpose).await? else {
            return Ok(Review::Runs);
        };
        Ok(
            match self
                .company_provider(tx, tenant_id, purpose, &settings)
                .await?
            {
                Ok(provider) => Review::Company(provider),
                Err(reason) => Review::Refused(reason),
            },
        )
    }

    /// The provider of the company's runs, if any; `Unusable` when the
    /// company's own cannot be used.
    pub async fn provider_for(
        &self,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<Arc<dyn LlmProvider>>> {
        match &self.resolve(tenant_id).await?.runs {
            Resolution::Platform(provider) => Ok(provider.clone()),
            Resolution::Company(provider) => Ok(Some(provider.clone())),
            Resolution::Refused(reason) => Err(Unusable(reason.clone()).into()),
        }
    }

    /// The provider the company chose apart to review its answers, if it
    /// can be used: without it, its runs' model reviews them (ADR-108).
    pub async fn reviewer_for(
        &self,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<Arc<dyn LlmProvider>>> {
        Ok(match &self.resolve(tenant_id).await?.review {
            Review::Company(provider) => Some(provider.clone()),
            Review::Runs | Review::Refused(_) => None,
        })
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

    /// After a change of the company's settings, for its runs or for
    /// reviews: loaded again, here and in the other processes.
    pub fn invalidate(&self, tenant_id: Uuid) {
        self.forget(tenant_id);
        self.invalidations.announce(Invalidation::Llm(tenant_id));
    }

    /// Loaded again here: another process changed the settings.
    pub(crate) fn forget(&self, tenant_id: Uuid) {
        self.cache.invalidate(tenant_id);
    }

    pub(crate) fn forget_all(&self) {
        self.cache.invalidate_all();
    }
}
