//! The company's conversation gateways (`connector:manage`, company-wide,
//! ADR-125): listed with what Slack said of each and how many people linked
//! their identity, declared, changed and deleted.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_core::gateways::slack::{BOT_EVENTS, BOT_SCOPES};
use mimorya_db::repositories::gateway_identities::count_gateway_identities;
use mimorya_db::repositories::gateways::list_gateways;

use crate::app::AppState;
use crate::contract::admin::{GatewayUpdate, GatewayView, Gateways, NewSlackGateway};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId, iso};
use crate::plugins::Auth;
use crate::services::admin::{
    GatewayChanges, NewSlackGatewayInput, change_gateway, declare_slack_gateway, remove_gateway,
};
use crate::services::gateways::{bot_token_name, signing_secret_name};

const PERMISSION: &str = "connector:manage";

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<Gateways>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let secrets = state.gateways.secrets();
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_gateways(&mut tx).await?;
    let mut gateways = Vec::with_capacity(rows.len());
    for gateway in rows {
        let secrets_stored = secrets.exists(&mut tx, &bot_token_name(gateway.id)).await?
            && secrets
                .exists(&mut tx, &signing_secret_name(gateway.id))
                .await?;
        gateways.push(GatewayView {
            events_url: state.gateways.events_url(gateway.id),
            identities: count_gateway_identities(&mut tx, gateway.id).await?,
            secrets_stored,
            id: gateway.id,
            kind: gateway.kind,
            name: gateway.name,
            enabled: gateway.enabled,
            workspace_id: gateway.workspace_id,
            workspace_name: gateway.workspace_name,
            bot_user_id: gateway.bot_user_id,
            created_at: iso(&gateway.created_at),
            updated_at: iso(&gateway.updated_at),
        });
    }
    tx.commit().await?;
    Ok(Json(Gateways {
        gateways,
        slack_scopes: BOT_SCOPES.iter().map(|s| (*s).to_owned()).collect(),
        slack_events: BOT_EVENTS.iter().map(|e| (*e).to_owned()).collect(),
    }))
}

/// The company's Slack application, checked with Slack.
pub async fn create_slack(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewSlackGateway>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let input = NewSlackGatewayInput {
        name: body.name,
        bot_token: body.bot_token,
        signing_secret: body.signing_secret,
        enabled: body.enabled.unwrap_or(true),
    };
    let id = declare_slack_gateway(&state.gateways, &access, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<GatewayUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let changes = GatewayChanges {
        name: body.name,
        enabled: body.enabled,
        bot_token: body.bot_token,
        signing_secret: body.signing_secret,
    };
    change_gateway(&state.gateways, &access, id, changes).await?;
    Ok(Json(Done::OK))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    remove_gateway(&state.gateways, &access, id).await?;
    Ok(Json(Done::OK))
}
