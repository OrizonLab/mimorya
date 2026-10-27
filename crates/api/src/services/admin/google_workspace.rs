//! A company's Google Workspace client, declared by its connector
//! administrators (ADR-121): its OAuth client ID, its secret sealed in the
//! secret store (never shown nor audited), one per company, under the
//! administration lock. Its people then link their own Workspace accounts
//! (`services::account_links`). Its name, activation and secret change as
//! any connector's (`connectors`), as does whether its tools may write; a
//! link goes with the connector deleted.

use mimorya_connectors::google_workspace::GoogleWorkspaceConfig;
use mimorya_connectors::outbound::token_problem;
use mimorya_core::DomainError;
use mimorya_core::account_links::{
    GOOGLE_CALENDAR, GOOGLE_GMAIL, GOOGLE_WORKSPACE, google_workspace_application,
};
use mimorya_db::is_unique_violation;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{
    NewConnectorInstance, create_connector_instance, list_connector_instances,
};
use serde_json::json;
use uuid::Uuid;

use super::connectors::{ConnectorAdmin, PERMISSION};
use super::{audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::connectors::token_secret_name;

pub struct NewGoogleWorkspace {
    pub name: String,
    pub client_id: String,
    pub client_secret: String,
    pub enabled: bool,
    /// Its tools may send e-mail and create events.
    pub write_tools: bool,
}

const ALREADY: &str = "L’application Google Workspace de l’entreprise est déjà déclarée : \
     modifiez-la, ou supprimez-la avant d’en déclarer une autre";

pub async fn create_google_workspace_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    input: NewGoogleWorkspace,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let application = google_workspace_application(&input.client_id, &input.client_secret)
        .map_err(DomainError::validation)?;
    // As any connector's secret, written as a bearer token is.
    if let Some(problem) = token_problem(&input.client_secret) {
        return Err(DomainError::validation(problem).into());
    }
    admin.secrets.require()?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    let declared = list_connector_instances(&mut tx, false)
        .await?
        .iter()
        .any(|instance| instance.connector_type == GOOGLE_WORKSPACE);
    if declared {
        return Err(DomainError::conflict(ALREADY).into());
    }
    let config = serde_json::to_value(GoogleWorkspaceConfig {
        client_id: application.client_id.clone(),
        issuer: application.issuer,
        gmail: GOOGLE_GMAIL.into(),
        calendar: GOOGLE_CALENDAR.into(),
        write_tools: input.write_tools,
    })?;
    let created = create_connector_instance(
        &mut tx,
        NewConnectorInstance {
            tenant_id,
            connector_type: GOOGLE_WORKSPACE,
            name: &input.name,
            enabled: input.enabled,
            config: &config,
        },
    )
    .await;
    let id = match created {
        Ok(id) => id,
        // The name, or another declaration made meanwhile.
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict(
                "Un connecteur porte déjà ce nom, ou l’application Google Workspace est déjà \
                 déclarée",
            )
            .into());
        }
        Err(error) => return Err(error.into()),
    };
    admin
        .secrets
        .put(
            &mut tx,
            tenant_id,
            &token_secret_name(id),
            &input.client_secret,
        )
        .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "connector.created", "connector", id).metadata(json!({
            "name": input.name,
            "type": GOOGLE_WORKSPACE,
            "clientId": application.client_id,
            "enabled": input.enabled,
            "writeTools": input.write_tools,
            "tokenStored": true,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}
