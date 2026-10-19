//! The company's model provider (`settings:manage`, company-wide): what its
//! runs use, the choice of its own provider, going back to the platform's,
//! and a connection test. The API key is write-only (ADR-024).

use axum::Json;
use axum::extract::State;
use mimorya_agent::llm::{PROVIDER_KINDS, default_model, requires_api_key, uses_listed_endpoint};
use mimorya_db::repositories::llm_settings::get_llm_settings;
use serde::Deserialize;
use serde_json::{Value, json};
use validator::Validate;

use crate::app::AppState;
use crate::http::{ApiResult, Body, iso, trimmed, trimmed_opt};
use crate::plugins::Auth;
use crate::secrets::LLM_API_KEY;
use crate::services::admin::{LlmInput, configure_llm, reset_llm, test_llm};
use crate::services::llm::Resolution;

const PERMISSION: &str = "settings:manage";

pub async fn show(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
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
    let settings = settings.map(|s| {
        json!({
            "provider": s.provider,
            "model": s.model,
            "baseUrl": s.base_url,
            "keyStored": key_stored,
            "priced": prices.has(&s.model),
            "usable": problem.is_none(),
            "problem": problem,
            "updatedAt": iso(&s.updated_at),
        })
    });
    let platform = state.llm.platform().map(
        |p| json!({ "provider": p.id(), "model": p.model(), "priced": prices.has(p.model()) }),
    );
    let providers: Vec<Value> = PROVIDER_KINDS
        .iter()
        .map(|kind| {
            json!({
                "kind": kind,
                "requiresKey": requires_api_key(kind),
                "listedEndpoint": uses_listed_endpoint(kind),
                "defaultModel": default_model(kind),
            })
        })
        .collect();
    Ok(Json(json!({
        "secretsAvailable": state.llm.secrets().available(),
        "platform": platform,
        "settings": settings,
        "providers": providers,
        "endpoints": state.llm.listed_endpoints(),
    })))
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmBody {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 40))]
    provider: String,
    #[serde(default, deserialize_with = "trimmed_opt")]
    model: Option<String>,
    #[serde(default, deserialize_with = "trimmed_opt")]
    base_url: Option<String>,
    /// Omitted: the stored key is kept (same endpoint only).
    #[serde(default, deserialize_with = "trimmed_opt")]
    api_key: Option<String>,
}

pub async fn configure(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<LlmBody>,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    let input = LlmInput {
        provider: body.provider,
        model: body.model.filter(|m| !m.is_empty()),
        base_url: body.base_url.filter(|u| !u.is_empty()),
        api_key: body.api_key.filter(|k| !k.is_empty()),
    };
    configure_llm(&state.db, &state.llm, &access, input).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn reset(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    reset_llm(&state.db, &state.llm, &access).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn test(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    let outcome = test_llm(&state.db, &state.llm, state.llm.prices(), &access).await?;
    Ok(Json(json!({
        "ok": outcome.error.is_none(),
        "provider": outcome.provider,
        "model": outcome.model,
        "error": outcome.error,
    })))
}
