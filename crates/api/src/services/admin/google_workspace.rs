//! A company's Google Workspace client, declared by its connector
//! administrators (ADR-121): its OAuth client ID, checked here, and its
//! secret, declared as any application people link their accounts at
//! (`link_applications`). Its people then link their own Workspace
//! accounts (`services::account_links`). Its name, activation and secret
//! change as any connector's (`connectors`), as does whether its tools may
//! write; a link goes with the connector deleted.

use mimorya_connectors::google_workspace::GoogleWorkspaceConfig;
use mimorya_core::DomainError;
use mimorya_core::account_links::{
    GOOGLE_CALENDAR, GOOGLE_GMAIL, LinkKind, google_workspace_application,
};
use serde_json::{Map, json};
use uuid::Uuid;

use super::connectors::{ConnectorAdmin, PERMISSION};
use super::link_applications::{Declaration, declare_application};
use crate::services::AccessContext;

pub struct NewGoogleWorkspace {
    pub name: String,
    pub client_id: String,
    pub client_secret: String,
    pub enabled: bool,
    /// Its tools may send e-mail and create events.
    pub write_tools: bool,
}

pub async fn create_google_workspace_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    input: NewGoogleWorkspace,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let application = google_workspace_application(&input.client_id, &input.client_secret)
        .map_err(DomainError::validation)?;
    let config = serde_json::to_value(GoogleWorkspaceConfig {
        client_id: application.client_id.clone(),
        issuer: application.issuer,
        gmail: GOOGLE_GMAIL.into(),
        calendar: GOOGLE_CALENDAR.into(),
        write_tools: input.write_tools,
    })?;
    let declaration = Declaration {
        kind: LinkKind::GoogleWorkspace,
        name: input.name,
        enabled: input.enabled,
        config,
        secret: input.client_secret,
        audited: Map::from_iter([
            ("clientId".into(), json!(application.client_id)),
            ("writeTools".into(), json!(input.write_tools)),
        ]),
    };
    declare_application(admin, access, declaration).await
}
