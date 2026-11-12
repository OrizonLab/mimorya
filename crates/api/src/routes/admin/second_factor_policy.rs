//! What the company requires of its password accounts (`settings:manage`,
//! company-wide, ADR-201): read, and set where the server can keep a second
//! factor.

use axum::Json;
use axum::extract::State;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{SecondFactorPolicyUpdate, SecondFactorPolicyView};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::secrets::SecretStore;
use crate::services::admin::{company_second_factor_policy, set_company_second_factor_policy};

fn secrets(state: &AppState) -> SecretStore {
    SecretStore::new(state.config.secrets.clone())
}

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<SecondFactorPolicyView>> {
    let access = state.access.context(identity).await?;
    let requirement = company_second_factor_policy(&state.db, &access).await?;
    Ok(Json(SecondFactorPolicyView {
        requirement: requirement.into(),
        secrets_available: secrets(&state).available(),
    }))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<SecondFactorPolicyUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    set_company_second_factor_policy(
        &state.db,
        &secrets(&state),
        &access,
        body.requirement.into(),
    )
    .await?;
    Ok(Json(Done::OK))
}
