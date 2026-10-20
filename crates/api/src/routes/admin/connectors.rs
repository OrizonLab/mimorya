//! Connectors (`connector:manage`, company-wide, ADR-025): reading them with
//! their state, trying an MCP server before adding it (ADR-050), adding it
//! over HTTPS, changing it (address, token, per-tool policies, activation),
//! deleting it and testing it. Tokens are write-only.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_connectors::connector_types;
use mimorya_db::repositories::connectors::list_connector_instances;

use crate::app::AppState;
use crate::contract::admin::{
    ConnectorTest, ConnectorTrial, ConnectorUpdate, ConnectorView, Connectors, DelegablePermission,
    McpEndpoint, NewConnector, risks,
};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::secrets::SecretStore;
use crate::services::admin::{
    ConnectorAdmin, ConnectorChanges, McpTrial, NewMcpConnector, TokenChange, TrialServices,
    create_mcp_connector, delete_connector, http_config, test_connector, try_mcp_connector,
    update_connector,
};
use crate::services::connectors::token_secret_name;
use crate::services::permission_catalog::permission_catalog;

const PERMISSION: &str = "connector:manage";
fn secrets(state: &AppState) -> SecretStore {
    SecretStore::new(state.config.secrets.clone())
}

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<Connectors>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let store = secrets(&state);
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let instances = list_connector_instances(&mut tx, false).await?;
    let mut views = Vec::with_capacity(instances.len());
    for i in &instances {
        let mcp = match http_config(i) {
            Some((prefix, url)) => Some(McpEndpoint {
                prefix,
                url,
                token_stored: store.exists(&mut tx, &token_secret_name(i.id)).await?,
            }),
            None => None,
        };
        views.push(ConnectorView::from_row(i, mcp)?);
    }
    tx.commit().await?;
    let types = connector_types()
        .iter()
        .map(|c| (&c.info()).into())
        .collect();
    // What a tool policy may require: permissions an agent can hold.
    let permissions = permission_catalog()
        .iter()
        .filter(|p| p.delegable)
        .map(|p| DelegablePermission {
            permission: p.permission.into(),
            label: p.label.into(),
        })
        .collect();
    Ok(Json(Connectors {
        instances: views,
        types,
        permissions,
        risks: risks(),
        secrets_available: store.available(),
        allow_private_network: state.connectors.network().allow_private_network,
    }))
}

fn admin<'a>(
    state: &'a AppState,
    store: &'a SecretStore,
    catalog: &'a [mimorya_core::authz::PermissionInfo],
) -> ConnectorAdmin<'a> {
    ConnectorAdmin {
        db: &state.db,
        connectors: &state.connectors,
        secrets: store,
        catalog,
    }
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewConnector>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let input = NewMcpConnector {
        name: body.name,
        prefix: body.prefix,
        url: body.url,
        token: body.token.filter(|t| !t.is_empty()),
        enabled: body.enabled.unwrap_or(true),
    };
    let id = create_mcp_connector(&admin(&state, &store, &catalog), &access, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<ConnectorUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let changes = ConnectorChanges {
        name: body.name,
        enabled: body.enabled,
        url: body.url,
        token: body
            .token
            .map(|token| match token.filter(|t| !t.is_empty()) {
                Some(value) => TokenChange::Replace(value),
                None => TokenChange::Remove,
            }),
        tool_policies: body.tool_policies.map(|policies| {
            policies
                .into_iter()
                .map(|(tool, policy)| (tool, policy.into()))
                .collect()
        }),
    };
    update_connector(&admin(&state, &store, &catalog), &access, id, changes).await?;
    Ok(Json(Done::OK))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    delete_connector(&admin(&state, &store, &catalog), &access, id).await?;
    Ok(Json(Done::OK))
}

pub async fn test(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<ConnectorTest>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let outcome = test_connector(&admin(&state, &store, &catalog), &access, id).await?;
    Ok(Json((&outcome).into()))
}

/// An MCP server tried before it is added: nothing is stored (ADR-050).
pub async fn trial(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<ConnectorTrial>,
) -> ApiResult<Json<ConnectorTest>> {
    let access = state.access.context(identity).await?;
    let services = TrialServices {
        db: state.db.clone(),
        connectors: state.connectors.clone(),
        background: state.background.clone(),
    };
    let trial = McpTrial {
        prefix: body.prefix,
        url: body.url,
        token: body.token.filter(|t| !t.is_empty()),
    };
    let outcome = try_mcp_connector(services, &access, trial).await?;
    Ok(Json((&outcome).into()))
}
