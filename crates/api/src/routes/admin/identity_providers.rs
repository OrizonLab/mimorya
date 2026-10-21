//! The company's identity providers (`settings:manage`, company-wide,
//! ADR-063): reading them with the address to register at the provider,
//! declaring one, changing it, removing it. Client secrets are write-only.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_db::repositories::identity_providers::list_identity_providers;

use crate::app::AppState;
use crate::contract::admin::{
    IdentityProviderUpdate, IdentityProviderView, IdentityProvidersView, NewIdentityProvider,
};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::secrets::SecretStore;
use crate::services::admin::{
    ProviderChanges, ProviderDeclaration, change_identity_provider, declare_identity_provider,
    remove_identity_provider,
};
use crate::services::sso::client_secret_name;

const PERMISSION: &str = "settings:manage";

fn secrets(state: &AppState) -> SecretStore {
    SecretStore::new(state.config.secrets.clone())
}

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<IdentityProvidersView>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let store = secrets(&state);
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_identity_providers(&mut tx).await?;
    let mut providers = Vec::with_capacity(rows.len());
    for row in rows {
        let stored = store.exists(&mut tx, &client_secret_name(row.id)).await?;
        let link = state.sso.sign_in_link(row.id);
        providers.push(IdentityProviderView::new(row, link, stored));
    }
    tx.commit().await?;
    Ok(Json(IdentityProvidersView {
        secrets_available: store.available(),
        redirect_uri: state.sso.redirect_uri().into(),
        providers,
    }))
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewIdentityProvider>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let declaration = ProviderDeclaration {
        kind: body.kind,
        name: body.name,
        directory_id: body.directory_id,
        issuer: body.issuer,
        client_id: body.client_id,
        client_secret: body.client_secret,
        domains: body.domains,
        enabled: body.enabled.unwrap_or(true),
        required: body.required.unwrap_or(false),
        create_accounts: body.create_accounts.unwrap_or(false),
    };
    let id = declare_identity_provider(
        &state.db,
        &state.sso,
        &secrets(&state),
        &access,
        declaration,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<IdentityProviderUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let changes = ProviderChanges {
        name: body.name,
        client_secret: body.client_secret,
        domains: body.domains,
        enabled: body.enabled,
        required: body.required,
        create_accounts: body.create_accounts,
    };
    change_identity_provider(&state.db, &secrets(&state), &access, id, changes).await?;
    Ok(Json(Done::OK))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    remove_identity_provider(&state.db, &secrets(&state), &access, id).await?;
    Ok(Json(Done::OK))
}
