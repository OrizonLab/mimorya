//! The company's embedding model (`settings:manage`, company-wide, ADR-074):
//! its choice, how far its memory is indexed, going back to searching by
//! words only, and a connection test. The API key is write-only.

use axum::Json;
use axum::extract::State;
use mimorya_agent::llm::embedding::{
    EMBEDDING_KINDS, default_embedding_model, default_min_similarity, embedding_requires_key,
    embedding_uses_listed_endpoint,
};
use mimorya_db::repositories::embedding_settings::get_embedding_settings;
use mimorya_db::repositories::memory_embeddings::index_progress;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{
    EmbeddingChoice, EmbeddingKind, EmbeddingSettingsView, EmbeddingTestResult, EmbeddingView,
};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::secrets::EMBEDDING_API_KEY;
use crate::services::admin::{
    EmbeddingInput, configure_embedding, reset_embedding, test_embedding,
};
use crate::services::embeddings::EmbeddingResolution;

const PERMISSION: &str = "settings:manage";

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<EmbeddingView>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let embeddings = &state.embeddings;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let settings = get_embedding_settings(&mut tx).await?;
    let key_stored = embeddings
        .secrets()
        .exists(&mut tx, EMBEDDING_API_KEY)
        .await?;
    let progress = match &settings {
        Some(s) => Some(index_progress(&mut tx, &s.model).await?),
        None => None,
    };
    tx.commit().await?;
    let problem = match embeddings.resolve(identity.tenant_id).await?.as_ref() {
        EmbeddingResolution::Refused(reason) => Some(reason.clone()),
        _ => None,
    };
    let failure = embeddings.indexing_failure(identity.tenant_id);
    let settings = settings.zip(progress).map(|(s, progress)| {
        EmbeddingSettingsView::new(&s, key_stored, problem, progress, failure)
    });
    let providers = EMBEDDING_KINDS
        .iter()
        .map(|kind| EmbeddingKind {
            kind: (*kind).into(),
            requires_key: embedding_requires_key(kind),
            listed_endpoint: embedding_uses_listed_endpoint(kind),
            default_model: default_embedding_model(kind).map(Into::into),
            default_min_similarity: default_min_similarity(kind),
        })
        .collect();
    Ok(Json(EmbeddingView {
        secrets_available: embeddings.secrets().available(),
        settings,
        providers,
        endpoints: embeddings.listed_endpoints().to_vec(),
    }))
}

pub async fn configure(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<EmbeddingChoice>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let input = EmbeddingInput {
        provider: body.provider,
        model: body.model.filter(|m| !m.is_empty()),
        base_url: body.base_url.filter(|u| !u.is_empty()),
        min_similarity: body.min_similarity,
        api_key: body.api_key.filter(|k| !k.is_empty()),
    };
    configure_embedding(&state.db, &state.embeddings, &access, input).await?;
    Ok(Json(Done::OK))
}

pub async fn reset(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    reset_embedding(&state.db, &state.embeddings, &access).await?;
    Ok(Json(Done::OK))
}

pub async fn test(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<EmbeddingTestResult>> {
    let access = state.access.context(identity).await?;
    let outcome = test_embedding(&state.db, &state.embeddings, &access).await?;
    Ok(Json(outcome.into()))
}
