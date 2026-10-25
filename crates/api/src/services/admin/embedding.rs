//! The company's embedding model (ADR-074): chosen, changed or removed by
//! an administrator of its settings, under the administration lock.
//! Choosing one sends the company's memory and documents' passages to it
//! (ADR-096); removing it wipes the vectors. The API key is sealed in the secret store, never returned nor
//! audited, and never carried from one endpoint to another.

use mimorya_agent::llm::api_key_problem;
use mimorya_agent::llm::embedding::{check_tenant_embedding, embedding_requires_key};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::document_embeddings::clear_passage_embeddings;
use mimorya_db::repositories::embedding_settings::{
    NewEmbeddingSettings, delete_embedding_settings, get_embedding_settings, put_embedding_settings,
};
use mimorya_db::repositories::memory_embeddings::clear_embeddings;
use serde_json::json;

use super::{audit_entry, begin_change};
use crate::secrets::EMBEDDING_API_KEY;
use crate::services::AccessContext;
use crate::services::embeddings::{EmbeddingResolution, EmbeddingService, UsageBy};

const PERMISSION: &str = "settings:manage";
const TEST_TEXT: &str = "Test de connexion";

pub struct EmbeddingInput {
    pub provider: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// `None`: the provider's starting value.
    pub min_similarity: Option<f32>,
    /// `None` keeps the stored key, which only ever serves the same endpoint.
    pub api_key: Option<String>,
}

pub async fn configure_embedding(
    db: &Database,
    embeddings: &EmbeddingService,
    access: &AccessContext,
    input: EmbeddingInput,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let choice = check_tenant_embedding(
        &input.provider,
        input.model.as_deref(),
        input.base_url.as_deref(),
        input.min_similarity,
        embeddings.listed_endpoints(),
    )
    .map_err(DomainError::validation)?;
    if let Some(problem) = input.api_key.as_deref().and_then(api_key_problem) {
        return Err(DomainError::validation(problem).into());
    }
    let secrets = embeddings.secrets();
    if input.api_key.is_some() {
        // Says why a key cannot be kept, before anything is written.
        secrets.require()?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let previous = get_embedding_settings(&mut tx).await?;
    let same_endpoint = previous
        .as_ref()
        .is_some_and(|p| p.provider == choice.provider && p.base_url == choice.base_url);
    let key_replaced = match &input.api_key {
        Some(key) => {
            secrets
                .put(&mut tx, tenant_id, EMBEDDING_API_KEY, key)
                .await?;
            true
        }
        None if same_endpoint => false,
        None => {
            // A key never follows the company to another endpoint.
            secrets.delete(&mut tx, EMBEDDING_API_KEY).await?;
            false
        }
    };
    if embedding_requires_key(&choice.provider)
        && !secrets.exists(&mut tx, EMBEDDING_API_KEY).await?
    {
        return Err(DomainError::validation(format!(
            "Saisissez la clé d’API de « {} » : une clé ne passe jamais d’un fournisseur \
             ou d’une adresse à l’autre",
            choice.provider
        ))
        .into());
    }
    put_embedding_settings(
        &mut tx,
        NewEmbeddingSettings {
            tenant_id,
            provider: &choice.provider,
            model: &choice.model,
            base_url: choice.base_url.as_deref(),
            min_similarity: choice.min_similarity,
            updated_by_user_id: access.subject.user_id,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "embedding.configured", "tenant", tenant_id).metadata(json!({
            "provider": choice.provider,
            "model": choice.model,
            "baseUrl": choice.base_url,
            "minSimilarity": choice.min_similarity,
            "keyReplaced": key_replaced,
            "previousProvider": previous.as_ref().map(|p| &p.provider),
            "previousModel": previous.as_ref().map(|p| &p.model),
        })),
    )
    .await?;
    tx.commit().await?;
    embeddings.invalidate(tenant_id);
    Ok(())
}

/// Back to searching by words only: the settings, the key and every vector
/// of the company's memory and documents are deleted.
pub async fn reset_embedding(
    db: &Database,
    embeddings: &EmbeddingService,
    access: &AccessContext,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let removed = delete_embedding_settings(&mut tx).await?;
    let key_removed = embeddings
        .secrets()
        .delete(&mut tx, EMBEDDING_API_KEY)
        .await?;
    if !removed && !key_removed {
        return Err(DomainError::not_found(
            "Votre entreprise cherche déjà dans sa mémoire par les mots seulement",
        )
        .into());
    }
    let cleared = clear_embeddings(&mut tx).await?;
    let passages_cleared = clear_passage_embeddings(&mut tx).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "embedding.reset", "tenant", tenant_id).metadata(json!({
            "vectorsCleared": cleared,
            "passageVectorsCleared": passages_cleared,
        })),
    )
    .await?;
    tx.commit().await?;
    embeddings.invalidate(tenant_id);
    Ok(())
}

pub struct EmbeddingTestOutcome {
    pub provider: String,
    pub model: String,
    /// The provider's refusal, as shown to users (never the key).
    pub error: Option<String>,
}

/// Embeds a short sentence with the company's model, as its memory would
/// be; what it costs is recorded like any other use.
pub async fn test_embedding(
    db: &Database,
    embeddings: &EmbeddingService,
    access: &AccessContext,
) -> anyhow::Result<EmbeddingTestOutcome> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let embedder = match embeddings.resolve(tenant_id).await?.as_ref() {
        EmbeddingResolution::Ready(embedder) => embedder.clone(),
        EmbeddingResolution::Off => {
            return Err(
                DomainError::unavailable("Aucun modèle d’embeddings n’est configuré").into(),
            );
        }
        EmbeddingResolution::Refused(reason) => {
            return Err(DomainError::unavailable(format!(
                "La configuration enregistrée ne peut pas être utilisée : {reason}"
            ))
            .into());
        }
    };
    let error = match embedder.provider.embed(&[TEST_TEXT.to_string()]).await {
        Ok(embedded) => {
            let by = UsageBy {
                user_id: Some(access.subject.user_id),
                run_id: None,
            };
            embeddings
                .record_usage(tenant_id, &embedder, &embedded, by)
                .await?;
            None
        }
        Err(error) => Some(error.message),
    };
    let outcome = EmbeddingTestOutcome {
        provider: embedder.provider.id().to_string(),
        model: embedder.model.clone(),
        error,
    };
    let mut tx = db.begin(tenant_id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "embedding.tested", "tenant", tenant_id).metadata(json!({
            "provider": outcome.provider,
            "model": outcome.model,
            "ok": outcome.error.is_none(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(outcome)
}
