//! The company's conversation gateways (`connector:manage`, company-wide,
//! ADR-125): listed with what Slack said of each, its token's permissions
//! among it (ADR-132), and how many people linked their identity;
//! declared, changed, checked again with Slack and deleted. A Telegram bot
//! (ADR-133) and a Teams bot (ADR-138) alike.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::slack::{BOT_EVENTS, BOT_SCOPES, extra_scopes, missing_scopes};
use mimorya_db::repositories::gateway_identities::count_gateway_identities;
use mimorya_db::repositories::gateways::list_gateways;

use crate::app::AppState;
use crate::contract::admin::{
    GatewayUpdate, GatewayView, Gateways, NewSlackGateway, NewTeamsGateway, NewTelegramGateway,
};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId, iso};
use crate::plugins::Auth;
use crate::services::admin::{
    GatewayChanges, NewSlackGatewayInput, NewTeamsGatewayInput, NewTelegramGatewayInput,
    change_gateway, check_gateway, declare_slack_gateway, declare_teams_gateway,
    declare_telegram_gateway, remove_gateway,
};
use crate::services::gateways::{
    GatewayService, bot_token_name, client_secret_name, signing_secret_name, webhook_secret_name,
};
use uuid::Uuid;

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
        let (stored, events_url) = served(&state.gateways, &gateway.kind, gateway.id);
        let mut secrets_stored = true;
        for name in &stored {
            secrets_stored &= secrets.exists(&mut tx, name).await?;
        }
        let granted = gateway.granted_scopes.as_deref();
        gateways.push(GatewayView {
            missing_scopes: granted
                .map(|g| missing_scopes(g).into_iter().map(str::to_owned).collect()),
            extra_scopes: granted.map(extra_scopes),
            scopes_checked_at: gateway.scopes_checked_at.as_ref().map(iso),
            events_url,
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

/// The company's Telegram bot, checked with Telegram, its webhook given it
/// (ADR-133).
pub async fn create_telegram(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewTelegramGateway>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let input = NewTelegramGatewayInput {
        name: body.name,
        bot_token: body.bot_token,
        enabled: body.enabled.unwrap_or(true),
    };
    let id = declare_telegram_gateway(&state.gateways, &access, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

/// The company's Microsoft Teams bot, its client secret tried with
/// Microsoft Entra (ADR-138).
pub async fn create_teams(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewTeamsGateway>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let input = NewTeamsGatewayInput {
        name: body.name,
        app_id: body.app_id,
        directory_id: body.directory_id,
        client_secret: body.client_secret,
        enabled: body.enabled.unwrap_or(true),
    };
    let id = declare_teams_gateway(&state.gateways, &access, input).await?;
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
        client_secret: body.client_secret,
    };
    change_gateway(&state.gateways, &access, id, changes).await?;
    Ok(Json(Done::OK))
}

/// Slack asked again what the gateway's token is: its permissions among
/// it (ADR-132).
pub async fn check(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    check_gateway(&state.gateways, &access, id).await?;
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

/// The secrets a gateway of service `kind` keeps, and where its service
/// sends it what proves itself: a Slack application's events by its signing
/// secret, a Telegram bot's updates by its webhook's (ADR-133), a Teams
/// bot's activities by Microsoft's token, its client secret writing back
/// (ADR-138).
fn served(gateways: &GatewayService, kind: &str, id: Uuid) -> (Vec<String>, String) {
    match GatewayKind::of(kind) {
        Some(GatewayKind::Telegram) => (
            vec![bot_token_name(id), webhook_secret_name(id)],
            gateways.telegram_updates_url(id),
        ),
        Some(GatewayKind::Teams) => (
            vec![client_secret_name(id)],
            gateways.teams_activities_url(id),
        ),
        _ => (
            vec![bot_token_name(id), signing_secret_name(id)],
            gateways.events_url(id),
        ),
    }
}
