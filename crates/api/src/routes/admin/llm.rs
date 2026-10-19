//! The company's model provider (`settings:manage`, company-wide): what its
//! runs use, the choice of its own provider, going back to the platform's,
//! and a connection test. The API key is write-only (ADR-024).

use axum::Json;
use axum::extract::State;
use mimorya_agent::llm::{PROVIDER_KINDS, default_model, requires_api_key, uses_listed_endpoint};
use mimorya_db::repositories::llm_settings::get_llm_settings;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{
    LlmChoice, LlmSettingsView, LlmTestResult, LlmView, PlatformModel, ProviderKind,
};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::secrets::LLM_API_KEY;
use crate::services::admin::{LlmInput, configure_llm, reset_llm, test_llm};
use crate::services::llm::Resolution;

const PERMISSION: &str = "settings:manage";

pub async fn show(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<LlmView>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let settings = get_llm_settings(&mut tx).await?;
    let key_stored = state.llm.secrets().exists(&mut tx, LLM_API_KEY).await?;
    tx.commit().await?;
    let prices = state.llm.prices();
    let problem = match state.llm.resolve(identity.tenant_id).await?.as_ref() {
        Resolution::Refused(reason) => Some(reason.clone()),
        _ => None,
    };
    let settings = settings
        .map(|s| LlmSettingsView::new(&s, key_stored, prices.has(&s.model), problem.clone()));
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
        providers,
        endpoints: state.llm.listed_endpoints().to_vec(),
    }))
}

pub async fn configure(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<LlmChoice>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let input = LlmInput {
        provider: body.provider,
        model: body.model.filter(|m| !m.is_empty()),
        base_url: body.base_url.filter(|u| !u.is_empty()),
        api_key: body.api_key.filter(|k| !k.is_empty()),
    };
    configure_llm(&state.db, &state.llm, &access, input).await?;
    Ok(Json(Done::OK))
}

pub async fn reset(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    reset_llm(&state.db, &state.llm, &access).await?;
    Ok(Json(Done::OK))
}

pub async fn test(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<LlmTestResult>> {
    let access = state.access.context(identity).await?;
    let outcome = test_llm(&state.db, &state.llm, state.llm.prices(), &access).await?;
    Ok(Json(outcome.into()))
}
