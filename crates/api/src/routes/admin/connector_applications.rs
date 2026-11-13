//! What a company's people link their accounts at (`connector:manage`,
//! company-wide): its Microsoft 365 application (ADR-116), its Google
//! Workspace client (ADR-121) and its standard mail (ADR-205), one of each.
//! They change and go as any connector (`connectors`).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_connectors::standard_mail::MailServerConfig;

use super::connectors::admin;
use crate::app::AppState;
use crate::contract::Created;
use crate::contract::admin::{
    NewGoogleWorkspaceConnector, NewMailServerConnector, NewMicrosoft365Connector,
};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::services::admin::{
    NewGoogleWorkspace, NewMailServer, NewMicrosoft365, create_google_workspace_connector,
    create_mail_server_connector, create_microsoft365_connector,
};
use crate::services::permission_catalog::permission_catalog;

/// The company's Microsoft 365 application (ADR-116).
pub async fn create_microsoft365(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewMicrosoft365Connector>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (state.secret_store(), permission_catalog());
    let input = NewMicrosoft365 {
        name: body.name,
        directory: body.directory,
        client_id: body.client_id,
        client_secret: body.client_secret,
        enabled: body.enabled.unwrap_or(true),
        write_tools: body.write_tools.unwrap_or(false),
    };
    let id =
        create_microsoft365_connector(&admin(&state, &store, &catalog), &access, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

/// The company's Google Workspace client (ADR-121).
pub async fn create_google_workspace(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewGoogleWorkspaceConnector>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (state.secret_store(), permission_catalog());
    let input = NewGoogleWorkspace {
        name: body.name,
        client_id: body.client_id,
        client_secret: body.client_secret,
        enabled: body.enabled.unwrap_or(true),
        write_tools: body.write_tools.unwrap_or(false),
    };
    let id =
        create_google_workspace_connector(&admin(&state, &store, &catalog), &access, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

/// The company's standard mail (ADR-205): no secret, each person signs in
/// with their own account.
pub async fn create_mail_server(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewMailServerConnector>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (state.secret_store(), permission_catalog());
    let input = NewMailServer {
        name: body.name,
        config: MailServerConfig {
            imap: body.imap.into(),
            smtp: body.smtp.into(),
            caldav: body.caldav.filter(|address| !address.is_empty()),
            write_tools: body.write_tools.unwrap_or(false),
        },
        enabled: body.enabled.unwrap_or(true),
    };
    let id = create_mail_server_connector(&admin(&state, &store, &catalog), &access, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}
