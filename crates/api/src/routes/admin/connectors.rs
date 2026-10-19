//! Connectors (`connector:manage`, company-wide, ADR-025): reading them with
//! their state, adding an MCP server over HTTPS, changing it (address, token,
//! per-tool policies, activation), deleting it and testing it. Tokens are
//! write-only.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_connectors::connector_types;
use mimorya_db::repositories::connectors::list_connector_instances;
use serde::Deserialize;
use validator::Validate;

use crate::app::AppState;
use crate::contract::admin::{
    ConnectorTest, ConnectorView, Connectors, DelegablePermission, McpEndpoint, risks,
};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId, trimmed, trimmed_nullable, trimmed_opt};
use crate::plugins::Auth;
use crate::secrets::SecretStore;
use crate::services::admin::{
    ConnectorAdmin, ConnectorChanges, NewMcpConnector, TokenChange, ToolPolicyInput,
    create_mcp_connector, delete_connector, http_config, test_connector, update_connector,
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

const fn enabled_by_default() -> bool {
    true
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateConnectorBody {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 80))]
    name: String,
    #[serde(deserialize_with = "trimmed")]
    prefix: String,
    #[serde(deserialize_with = "trimmed")]
    url: String,
    #[serde(default, deserialize_with = "trimmed_opt")]
    token: Option<String>,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
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
    Body(body): Body<CreateConnectorBody>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let input = NewMcpConnector {
        name: body.name,
        prefix: body.prefix,
        url: body.url,
        token: body.token.filter(|t| !t.is_empty()),
        enabled: body.enabled,
    };
    let id = create_mcp_connector(&admin(&state, &store, &catalog), &access, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateConnectorBody {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 2, max = 80))]
    name: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default, deserialize_with = "trimmed_opt")]
    url: Option<String>,
    /// A text replaces the token, `null` removes it.
    #[serde(default, deserialize_with = "trimmed_nullable")]
    token: Option<Option<String>>,
    #[serde(default)]
    tool_policies: Option<HashMap<String, ToolPolicyInput>>,
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<UpdateConnectorBody>,
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
        tool_policies: body.tool_policies,
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
