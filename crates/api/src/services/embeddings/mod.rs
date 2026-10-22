//! Searching a company's memory by meaning (ADR-074): the embedding model
//! it chose, if any, checked again at each load against the rules as they
//! stand (an endpoint the operator removed from the list is refused); the
//! vector of a request (`meaning.rs`); and the indexing of its memory in the
//! background (`indexer.rs`). A company without a model, or whose model
//! cannot serve, is searched by its words only: nothing else stops, and its
//! administrators see why.

mod indexer;
mod meaning;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mimorya_agent::llm::embedding::{company_embedding_config, create_embedder};
use mimorya_agent::llm::{EmbeddingConfig, EmbeddingProvider, PriceTable};
use mimorya_db::repositories::embedding_settings::get_embedding_settings;
use mimorya_db::{Database, TenantTx};
use tokio::sync::Notify;
use uuid::Uuid;

pub use meaning::{RequestMeaning, UsageBy};

use super::tenant_cache::TenantCache;
use crate::secrets::{EMBEDDING_API_KEY, SecretStore};

/// Builds an embedding model from a configuration (tests substitute one).
pub type EmbeddingFactory =
    Arc<dyn Fn(&EmbeddingConfig) -> anyhow::Result<Arc<dyn EmbeddingProvider>> + Send + Sync>;

pub fn default_embedding_factory() -> EmbeddingFactory {
    Arc::new(create_embedder)
}

/// The model a company's memory is embedded with.
#[derive(Clone)]
pub struct CompanyEmbedder {
    pub provider: Arc<dyn EmbeddingProvider>,
    /// As the company chose it: the vectors it gives are stored under it.
    pub model: String,
    pub min_similarity: f32,
}

/// How a company's memory is searched.
#[derive(Clone)]
pub enum EmbeddingResolution {
    /// By its words only: the company chose no model.
    Off,
    Ready(CompanyEmbedder),
    /// By its words only: the model it chose cannot serve; why, in French.
    Refused(String),
}

pub struct EmbeddingService {
    db: Database,
    secrets: SecretStore,
    listed_endpoints: Vec<String>,
    factory: EmbeddingFactory,
    prices: Arc<PriceTable>,
    cache: TenantCache<EmbeddingResolution>,
    /// Memories to embed: a pass of the indexer is due.
    wake: Notify,
    /// One pass at a time: a memory is embedded once.
    indexing: tokio::sync::Mutex<()>,
    /// Why the last pass failed in each company (French), until one succeeds.
    failures: Mutex<HashMap<Uuid, String>>,
}

pub struct EmbeddingServiceOptions {
    pub secrets: SecretStore,
    /// The endpoints the operator lists (`MIMORYA_TENANT_LLM_ENDPOINTS`),
    /// checked again at each load.
    pub listed_endpoints: Vec<String>,
    pub factory: EmbeddingFactory,
    pub prices: Arc<PriceTable>,
}

impl EmbeddingService {
    pub fn new(db: Database, options: EmbeddingServiceOptions) -> Self {
        Self {
            db,
            secrets: options.secrets,
            listed_endpoints: options.listed_endpoints,
            factory: options.factory,
            prices: options.prices,
            cache: TenantCache::new(Duration::from_secs(60)),
            wake: Notify::new(),
            indexing: tokio::sync::Mutex::new(()),
            failures: Mutex::new(HashMap::new()),
        }
    }

    pub fn secrets(&self) -> &SecretStore {
        &self.secrets
    }

    pub fn listed_endpoints(&self) -> &[String] {
        &self.listed_endpoints
    }

    pub fn prices(&self) -> &Arc<PriceTable> {
        &self.prices
    }

    /// How the company's memory is searched, a refusal included (cached like
    /// the rest); an error only when the server itself fails.
    pub async fn resolve(&self, tenant_id: Uuid) -> anyhow::Result<Arc<EmbeddingResolution>> {
        let generation = match self.cache.get(tenant_id) {
            Ok(resolution) => return Ok(resolution),
            Err(generation) => generation,
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let resolution = Arc::new(self.load(&mut tx, tenant_id).await?);
        tx.commit().await?;
        if let EmbeddingResolution::Refused(reason) = resolution.as_ref() {
            tracing::warn!(%tenant_id, reason = %reason, "Company embedding model unusable");
        }
        self.cache.store(tenant_id, generation, resolution.clone());
        Ok(resolution)
    }

    async fn load(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
    ) -> anyhow::Result<EmbeddingResolution> {
        let Some(settings) = get_embedding_settings(tx).await? else {
            return Ok(EmbeddingResolution::Off);
        };
        let read = self.secrets.read(tx, tenant_id, EMBEDDING_API_KEY).await?;
        let api_key = match read.into_key() {
            Ok(key) => key,
            Err(reason) => return Ok(EmbeddingResolution::Refused(reason)),
        };
        let config = match company_embedding_config(
            &settings.provider,
            &settings.model,
            settings.base_url.as_deref(),
            api_key,
            &self.listed_endpoints,
        ) {
            Ok(config) => config,
            Err(reason) => return Ok(EmbeddingResolution::Refused(reason)),
        };
        Ok(match (self.factory)(&config) {
            Ok(provider) => EmbeddingResolution::Ready(CompanyEmbedder {
                provider,
                model: settings.model,
                min_similarity: settings.min_similarity,
            }),
            Err(_) => EmbeddingResolution::Refused(
                "Le modèle d’embeddings ne peut pas être préparé avec cette configuration".into(),
            ),
        })
    }

    /// The company's model, when it has one that can serve.
    pub async fn embedder(&self, tenant_id: Uuid) -> anyhow::Result<Option<CompanyEmbedder>> {
        Ok(match self.resolve(tenant_id).await?.as_ref() {
            EmbeddingResolution::Ready(embedder) => Some(embedder.clone()),
            EmbeddingResolution::Off | EmbeddingResolution::Refused(_) => None,
        })
    }

    /// After a change of the company's settings: loaded again, and its
    /// memory indexed for the new model.
    pub fn invalidate(&self, tenant_id: Uuid) {
        self.cache.invalidate(tenant_id);
        self.failed(tenant_id, None);
        self.wake();
    }

    /// A memory was written, or approved: a pass of the indexer is due.
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// Returns once a pass is due (at most one wake is kept meanwhile).
    pub async fn woken(&self) {
        self.wake.notified().await;
    }

    /// Why the last pass failed in the company, if it did.
    pub fn indexing_failure(&self, tenant_id: Uuid) -> Option<String> {
        let failures = self.failures.lock().expect("indexing failures");
        failures.get(&tenant_id).cloned()
    }

    fn failed(&self, tenant_id: Uuid, reason: Option<String>) {
        let mut failures = self.failures.lock().expect("indexing failures");
        match reason {
            Some(reason) => failures.insert(tenant_id, reason),
            None => failures.remove(&tenant_id),
        };
    }
}
