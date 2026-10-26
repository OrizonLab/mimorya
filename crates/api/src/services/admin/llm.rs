//! The company's own model providers: the one of its conversations
//! (ADR-024) and the one it chose apart to review their answers (ADR-108),
//! each chosen, changed, removed or tested by an administrator of its
//! settings, under the administration lock. Each API key is sealed in the
//! secret store, never returned nor audited, and never carried from one
//! endpoint to another, nor from one purpose to the other.

use std::sync::Arc;

use mimorya_agent::llm::{
    LlmMessage, LlmProvider, LlmRequest, PriceTable, api_key_problem, check_tenant_llm,
    requires_api_key,
};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::llm_settings::{
    ModelPurpose, NewLlmSettings, delete_llm_settings, get_llm_settings, put_llm_settings,
};
use mimorya_db::repositories::usage::{NewUsage, append_usage};
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change};
use crate::services::llm::{Review, Unusable, api_key_secret};
use crate::services::{AccessContext, LlmService};

const PERMISSION: &str = "settings:manage";

/// The audit action of a `change` of the company's provider for `purpose`.
fn action(purpose: ModelPurpose, change: &str) -> String {
    match purpose {
        ModelPurpose::Conversations => format!("llm.{change}"),
        ModelPurpose::Verification => format!("verification_model.{change}"),
    }
}

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
    purpose: ModelPurpose,
    input: LlmInput,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let choice = check_tenant_llm(
        &input.provider,
        input.model.as_deref(),
        input.base_url.as_deref(),
        llm.listed_endpoints(),
    )
    .map_err(DomainError::validation)?;
    if let Some(problem) = input.api_key.as_deref().and_then(api_key_problem) {
        return Err(DomainError::validation(problem).into());
    }
    if input.api_key.is_some() {
        // Says why a key cannot be kept, before anything is written.
        llm.secrets().require()?;
    }
    let secret = api_key_secret(purpose);
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let previous = get_llm_settings(&mut tx, purpose).await?;
    let same_endpoint = previous
        .as_ref()
        .is_some_and(|p| p.provider == choice.provider && p.base_url == choice.base_url);
    let key_replaced = match &input.api_key {
        Some(key) => {
            llm.secrets().put(&mut tx, tenant_id, secret, key).await?;
            true
        }
        None if same_endpoint => false,
        None => {
            // A key never follows the company to another endpoint.
            llm.secrets().delete(&mut tx, secret).await?;
            false
        }
    };
    if requires_api_key(&choice.provider) && !llm.secrets().exists(&mut tx, secret).await? {
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
            purpose,
            provider: &choice.provider,
            model: &choice.model,
            base_url: choice.base_url.as_deref(),
            updated_by_user_id: access.subject.user_id,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, &action(purpose, "configured"), "tenant", tenant_id).metadata(json!({
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

/// Back to the platform's provider for the conversations, to the
/// conversations' for reviews; the stored key is deleted.
pub async fn reset_llm(
    db: &Database,
    llm: &LlmService,
    access: &AccessContext,
    purpose: ModelPurpose,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let removed = delete_llm_settings(&mut tx, purpose).await?;
    let key_removed = llm
        .secrets()
        .delete(&mut tx, api_key_secret(purpose))
        .await?;
    if !removed && !key_removed {
        return Err(DomainError::not_found(match purpose {
            ModelPurpose::Conversations => {
                "Votre entreprise utilise déjà le modèle de la plateforme"
            }
            ModelPurpose::Verification => {
                "Les réponses sont déjà relues par le modèle de vos conversations"
            }
        })
        .into());
    }
    append_audit(
        &mut tx,
        audit_entry(access, &action(purpose, "reset"), "tenant", tenant_id),
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

/// Sends a minimal request to the provider the company would use for
/// `purpose`; its few tokens are recorded like any other use.
pub async fn test_llm(
    db: &Database,
    llm: &LlmService,
    prices: &PriceTable,
    access: &AccessContext,
    purpose: ModelPurpose,
) -> anyhow::Result<LlmTestOutcome> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let provider = match purpose {
        ModelPurpose::Conversations => runs_provider(llm, tenant_id).await?,
        ModelPurpose::Verification => match &llm.resolve(tenant_id).await?.review {
            Review::Company(provider) => provider.clone(),
            Review::Runs => {
                return Err(DomainError::unavailable(
                    "Aucun modèle de contrôle n’est choisi : les réponses sont relues par le \
                     modèle de vos conversations",
                )
                .into());
            }
            Review::Refused(reason) => return Err(unusable(reason).into()),
        },
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
                    // Counted as runs count them: cached tokens are input too.
                    input_tokens: i32::try_from(response.usage.input_total()).unwrap_or(i32::MAX),
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
        audit_entry(access, &action(purpose, "tested"), "tenant", tenant_id).metadata(json!({
            "provider": result.provider,
            "model": result.model,
            "ok": result.error.is_none(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}

/// The provider of the company's runs, or why there is none to test.
async fn runs_provider(llm: &LlmService, tenant_id: Uuid) -> anyhow::Result<Arc<dyn LlmProvider>> {
    match llm.provider_for(tenant_id).await {
        Ok(Some(provider)) => Ok(provider),
        Ok(None) => Err(DomainError::unavailable("Aucun modèle n’est configuré").into()),
        Err(error) => match error.downcast::<Unusable>() {
            Ok(Unusable(reason)) => Err(unusable(&reason).into()),
            Err(error) => Err(error),
        },
    }
}

fn unusable(reason: &str) -> DomainError {
    DomainError::unavailable(format!(
        "La configuration enregistrée ne peut pas être utilisée : {reason}"
    ))
}
