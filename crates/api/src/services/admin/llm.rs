//! The company's own model provider (ADR-024): chosen, changed or removed
//! by an administrator of its settings, under the administration lock. The
//! API key is sealed in the secret store, never returned nor audited, and
//! never carried from one endpoint to another.

use mimorya_agent::llm::{
    LlmMessage, LlmRequest, PriceTable, api_key_problem, check_tenant_llm, requires_api_key,
};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::llm_settings::{
    NewLlmSettings, delete_llm_settings, get_llm_settings, put_llm_settings,
};
use mimorya_db::repositories::usage::{NewUsage, append_usage};
use serde_json::json;

use super::{audit_entry, begin_change};
use crate::secrets::LLM_API_KEY;
use crate::services::{AccessContext, LlmService};

const PERMISSION: &str = "settings:manage";

pub struct LlmInput {
    pub provider: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// `None` keeps the stored key, which only ever serves the same endpoint.
    pub api_key: Option<String>,
}

pub async fn configure_llm(
    db: &Database,
    llm: &LlmService,
    access: &AccessContext,
    listed_endpoints: &[String],
    input: LlmInput,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let choice = check_tenant_llm(
        &input.provider,
        input.model.as_deref(),
        input.base_url.as_deref(),
        listed_endpoints,
    )
    .map_err(DomainError::validation)?;
    if let Some(problem) = input.api_key.as_deref().and_then(api_key_problem) {
        return Err(DomainError::validation(problem).into());
    }
    if input.api_key.is_some() {
        // Says why a key cannot be kept, before anything is written.
        llm.secrets().require()?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let previous = get_llm_settings(&mut tx).await?;
    let same_endpoint = previous
        .as_ref()
        .is_some_and(|p| p.provider == choice.provider && p.base_url == choice.base_url);
    let key_replaced = match &input.api_key {
        Some(key) => {
            llm.secrets()
                .put(&mut tx, tenant_id, LLM_API_KEY, key)
                .await?;
            true
        }
        None if same_endpoint => false,
        None => {
            // A key never follows the company to another endpoint.
            llm.secrets().delete(&mut tx, LLM_API_KEY).await?;
            false
        }
    };
    if requires_api_key(&choice.provider) && !llm.secrets().exists(&mut tx, LLM_API_KEY).await? {
        return Err(DomainError::validation(format!(
            "Saisissez la clé d’API de « {} » : une clé ne passe jamais d’un fournisseur \
             ou d’une adresse à l’autre",
            choice.provider
        ))
        .into());
    }
    put_llm_settings(
        &mut tx,
        NewLlmSettings {
            tenant_id,
            provider: &choice.provider,
            model: &choice.model,
            base_url: choice.base_url.as_deref(),
            updated_by_user_id: access.subject.user_id,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "llm.configured", "tenant", tenant_id).metadata(json!({
            "provider": choice.provider,
            "model": choice.model,
            "baseUrl": choice.base_url,
            "keyReplaced": key_replaced,
            "previousProvider": previous.as_ref().map(|p| &p.provider),
            "previousModel": previous.as_ref().map(|p| &p.model),
        })),
    )
    .await?;
    tx.commit().await?;
    llm.invalidate(tenant_id);
    Ok(())
}

/// Back to the platform's provider; the stored key is deleted.
pub async fn reset_llm(
    db: &Database,
    llm: &LlmService,
    access: &AccessContext,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let removed = delete_llm_settings(&mut tx).await?;
    let key_removed = llm.secrets().delete(&mut tx, LLM_API_KEY).await?;
    if !removed && !key_removed {
        return Err(DomainError::not_found(
            "Votre entreprise utilise déjà le modèle de la plateforme",
        )
        .into());
    }
    append_audit(
        &mut tx,
        audit_entry(access, "llm.reset", "tenant", tenant_id),
    )
    .await?;
    tx.commit().await?;
    llm.invalidate(tenant_id);
    Ok(())
}

pub struct LlmTestOutcome {
    pub provider: String,
    pub model: String,
    /// The provider's refusal, as shown to users (never the key).
    pub error: Option<String>,
}

/// Sends a minimal request to the provider the company's runs would use; its
/// few tokens are recorded like any other use.
pub async fn test_llm(
    db: &Database,
    llm: &LlmService,
    prices: &PriceTable,
    access: &AccessContext,
) -> anyhow::Result<LlmTestOutcome> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let provider = match llm.provider_for(tenant_id).await {
        Ok(Some(provider)) => provider,
        Ok(None) => return Err(DomainError::unavailable("Aucun modèle n’est configuré").into()),
        Err(_) => {
            return Err(DomainError::unavailable(
                "La configuration enregistrée ne peut pas être utilisée (clé illisible ou \
                 fournisseur refusé) : enregistrez-la de nouveau",
            )
            .into());
        }
    };
    let request = LlmRequest {
        system: "Réponds uniquement « OK ».".into(),
        messages: vec![LlmMessage::user("Test de connexion")],
        tools: Vec::new(),
        max_output_tokens: Some(16),
        effort: None,
        on_text_delta: None,
    };
    let outcome = provider.complete(&request).await;
    let mut tx = db.begin(tenant_id).await?;
    let result = match outcome {
        Ok(response) => {
            append_usage(
                &mut tx,
                NewUsage {
                    tenant_id,
                    user_id: Some(access.subject.user_id),
                    run_id: None,
                    kind: "llm",
                    cost_micros: prices
                        .cost_micros(&response.model, &response.usage)
                        .unwrap_or(0),
                    provider: response.provider,
                    model: response.model.clone(),
                    input_tokens: i32::try_from(response.usage.input_tokens).unwrap_or(i32::MAX),
                    output_tokens: i32::try_from(response.usage.output_tokens).unwrap_or(i32::MAX),
                },
            )
            .await?;
            LlmTestOutcome {
                provider: provider.id().to_string(),
                model: response.model,
                error: None,
            }
        }
        Err(error) => LlmTestOutcome {
            provider: provider.id().to_string(),
            model: provider.model().to_string(),
            error: Some(error.message),
        },
    };
    append_audit(
        &mut tx,
        audit_entry(access, "llm.tested", "tenant", tenant_id).metadata(json!({
            "provider": result.provider,
            "model": result.model,
            "ok": result.error.is_none(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
