//! Connectors (`connector:manage`, company-wide, ADR-025): reading them with
//! their state, trying an MCP server before adding it (ADR-050), adding it
//! over HTTPS, adding a REST API (ADR-112) whose operations an OpenAPI
//! description may offer first (ADR-114), changing either (address, token,
//! per-tool policies, activation; a REST API's token header and operations),
//! deleting it and testing it. Tokens are write-only.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_connectors::connector_types;
use mimorya_connectors::rest::RestConfig;
use mimorya_db::repositories::connectors::list_connector_instances;

use crate::app::AppState;
use crate::contract::admin::{
    ConnectorTest, ConnectorTrial, ConnectorUpdate, ConnectorView, Connectors, DelegablePermission,
    McpEndpoint, NewConnector, NewRestConnector, OpenApiDescription, RestApi, RestImport, risks,
};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::secrets::SecretStore;
use crate::services::admin::NewRestConnector as RestInput;
use crate::services::admin::{
    ConnectorAdmin, ConnectorChanges, Editable, McpTrial, NewMcpConnector, RestChanges,
    TokenChange, TrialServices, create_mcp_connector, create_rest_connector, delete_connector,
    read_openapi, test_connector, try_mcp_connector, update_connector,
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
        let (mut mcp, mut rest) = (None, None);
        if let Some(editable) = Editable::of(i) {
            let token_stored = store.exists(&mut tx, &token_secret_name(i.id)).await?;
            match editable {
                Editable::Mcp { prefix, url } => {
                    mcp = Some(McpEndpoint {
                        prefix,
                        url,
                        token_stored,
                    });
                }
                Editable::Rest(config) => {
                    rest = Some(RestApi {
                        operations: config.operations.iter().map(Into::into).collect(),
                        prefix: config.prefix,
                        url: config.url,
                        token_header: config.token_header,
                        token_stored,
                    });
                }
            }
        }
        views.push(ConnectorView::from_row(i, mcp, rest)?);
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

/// A REST API and its operations (ADR-112).
pub async fn create_rest(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewRestConnector>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let input = RestInput {
        name: body.name,
        config: RestConfig {
            prefix: body.prefix,
            url: body.url,
            token_header: body.token_header.filter(|h| !h.is_empty()),
            operations: body.operations.into_iter().map(Into::into).collect(),
        },
        token: body.token.filter(|t| !t.is_empty()),
        enabled: body.enabled.unwrap_or(true),
    };
    let id = create_rest_connector(&admin(&state, &store, &catalog), &access, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

/// The operations an OpenAPI description offers (ADR-114); nothing stored.
pub async fn import_rest(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<OpenApiDescription>,
) -> ApiResult<Json<RestImport>> {
    let access = state.access.context(identity).await?;
    Ok(Json(read_openapi(&access, &body.document)?.into()))
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
        rest: (body.token_header.is_some() || body.operations.is_some()).then(|| RestChanges {
            token_header: body.token_header,
            operations: body
                .operations
                .map(|operations| operations.into_iter().map(Into::into).collect()),
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
