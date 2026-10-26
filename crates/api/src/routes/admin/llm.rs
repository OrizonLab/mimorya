//! The company's model providers (`settings:manage`, company-wide): what its
//! runs use, the choice of its own provider, going back to the platform's,
//! and a connection test (ADR-024); the same for the model it chooses apart
//! to review their answers, going back to its runs' (ADR-108). An API key is
//! write-only.

use axum::Json;
use axum::extract::State;
use mimorya_agent::llm::{PROVIDER_KINDS, default_model, requires_api_key, uses_listed_endpoint};
use mimorya_db::TenantTx;
use mimorya_db::repositories::llm_settings::{ModelPurpose, get_llm_settings};

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{
    LlmChoice, LlmSettingsView, LlmTestResult, LlmView, PlatformModel, ProviderKind,
};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::services::AuthIdentity;
use crate::services::admin::{LlmInput, configure_llm, reset_llm, test_llm};
use crate::services::llm::{Resolution, Review, api_key_secret};

const PERMISSION: &str = "settings:manage";

pub async fn show(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<LlmView>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let models = state.llm.resolve(identity.tenant_id).await?;
    let runs_problem = match &models.runs {
        Resolution::Refused(reason) => Some(reason.clone()),
        _ => None,
    };
    let review_problem = match &models.review {
        Review::Refused(reason) => Some(reason.clone()),
        _ => None,
    };
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let settings =
        settings_view(&state, &mut tx, ModelPurpose::Conversations, runs_problem).await?;
    let verification =
        settings_view(&state, &mut tx, ModelPurpose::Verification, review_problem).await?;
    tx.commit().await?;
    let prices = state.llm.prices();
    let platform = state.llm.platform().map(|p| PlatformModel {
        provider: p.id().into(),
        model: p.model().into(),
        priced: prices.has(p.model()),
    });
    let providers = PROVIDER_KINDS
        .iter()
        .map(|kind| ProviderKind {
            kind: (*kind).into(),
            requires_key: requires_api_key(kind),
            listed_endpoint: uses_listed_endpoint(kind),
            default_model: default_model(kind).map(Into::into),
        })
        .collect();
    Ok(Json(LlmView {
        secrets_available: state.llm.secrets().available(),
        platform,
        settings,
        verification,
        providers,
        endpoints: state.llm.listed_endpoints().to_vec(),
    }))
}

/// The company's own provider for `purpose`, if it chose one.
async fn settings_view(
    state: &AppState,
    tx: &mut TenantTx,
    purpose: ModelPurpose,
    problem: Option<String>,
) -> anyhow::Result<Option<LlmSettingsView>> {
    let Some(settings) = get_llm_settings(tx, purpose).await? else {
        return Ok(None);
    };
    let secrets = state.llm.secrets();
    let key_stored = secrets.exists(tx, api_key_secret(purpose)).await?;
    let priced = state.llm.prices().has(&settings.model);
    Ok(Some(LlmSettingsView::new(
        &settings, key_stored, priced, problem,
    )))
}

pub async fn configure(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<LlmChoice>,
) -> ApiResult<Json<Done>> {
    choose(state, identity, ModelPurpose::Conversations, body).await
}

pub async fn reset(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    reset_llm(&state.db, &state.llm, &access, ModelPurpose::Conversations).await?;
    Ok(Json(Done::OK))
}

pub async fn test(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<LlmTestResult>> {
    try_purpose(state, identity, ModelPurpose::Conversations).await
}

pub async fn configure_verification(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<LlmChoice>,
) -> ApiResult<Json<Done>> {
    choose(state, identity, ModelPurpose::Verification, body).await
}

pub async fn reset_verification(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    reset_llm(&state.db, &state.llm, &access, ModelPurpose::Verification).await?;
    Ok(Json(Done::OK))
}

pub async fn test_verification(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<LlmTestResult>> {
    try_purpose(state, identity, ModelPurpose::Verification).await
}

async fn choose(
    state: AppState,
    identity: AuthIdentity,
    purpose: ModelPurpose,
    body: LlmChoice,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let input = LlmInput {
        provider: body.provider,
        model: body.model.filter(|m| !m.is_empty()),
        base_url: body.base_url.filter(|u| !u.is_empty()),
        api_key: body.api_key.filter(|k| !k.is_empty()),
    };
    configure_llm(&state.db, &state.llm, &access, purpose, input).await?;
    Ok(Json(Done::OK))
}

async fn try_purpose(
    state: AppState,
    identity: AuthIdentity,
    purpose: ModelPurpose,
) -> ApiResult<Json<LlmTestResult>> {
    let access = state.access.context(identity).await?;
    let outcome = test_llm(&state.db, &state.llm, state.llm.prices(), &access, purpose).await?;
    Ok(Json(outcome.into()))
}
