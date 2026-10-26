//! A company's own provider, for its conversations or for reviewing their
//! answers (ADR-024, ADR-108), loaded from what it stored and checked again
//! against the rules as they stand (an endpoint the operator removed from
//! the list is refused).

use std::sync::Arc;

use mimorya_agent::llm::{LlmProvider, company_provider_config};
use mimorya_db::TenantTx;
use mimorya_db::repositories::llm_settings::{ModelPurpose, get_llm_settings};
use uuid::Uuid;

use super::LlmService;
use crate::secrets::{LLM_API_KEY, VERIFICATION_API_KEY};

/// The secret that holds the API key of the company's provider for `purpose`.
pub fn api_key_secret(purpose: ModelPurpose) -> &'static str {
    match purpose {
        ModelPurpose::Conversations => LLM_API_KEY,
        ModelPurpose::Verification => VERIFICATION_API_KEY,
    }
}

impl LlmService {
    /// The company's own provider for `purpose`, if it chose one: usable, or
    /// why not (in French, for its administrators); an error only when the
    /// server itself fails.
    pub(super) async fn company_choice(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
        purpose: ModelPurpose,
    ) -> anyhow::Result<Option<Result<Arc<dyn LlmProvider>, String>>> {
        let Some(settings) = get_llm_settings(tx, purpose).await? else {
            return Ok(None);
        };
        let api_key = match self
            .secrets
            .read(tx, tenant_id, api_key_secret(purpose))
            .await?
            .into_key()
        {
            Ok(key) => key,
            Err(reason) => return Ok(Some(Err(reason))),
        };
        let config = match company_provider_config(
            &settings.provider,
            &settings.model,
            settings.base_url.as_deref(),
            api_key,
            &self.listed_endpoints,
        ) {
            Ok(config) => config,
            Err(reason) => return Ok(Some(Err(reason))),
        };
        Ok(Some((self.factory)(&config).map_err(|_| {
            "Le fournisseur ne peut pas être préparé avec cette configuration".to_string()
        })))
    }
}
