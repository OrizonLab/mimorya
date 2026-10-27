//! A company's Microsoft 365 application, declared by its connector
//! administrators (ADR-116): its directory and client IDs, its secret
//! sealed in the secret store (never shown nor audited), one per company,
//! under the administration lock. Its people then link their own accounts
//! (`services::account_links`). Its name, activation and secret change as
//! any connector's (`connectors`); a link goes with the connector deleted.

use mimorya_connectors::microsoft365::Microsoft365Config;
use mimorya_connectors::outbound::token_problem;
use mimorya_core::DomainError;
use mimorya_core::account_links::{MICROSOFT_GRAPH, MICROSOFT365, microsoft365_application};
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

pub struct NewMicrosoft365 {
    pub name: String,
    pub directory: String,
    pub client_id: String,
    pub client_secret: String,
    pub enabled: bool,
}

const ALREADY: &str = "L’application Microsoft 365 de l’entreprise est déjà déclarée : \
     modifiez-la, ou supprimez-la avant d’en déclarer une autre";

pub async fn create_microsoft365_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    input: NewMicrosoft365,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let application =
        microsoft365_application(&input.directory, &input.client_id, &input.client_secret)
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
        .any(|instance| instance.connector_type == MICROSOFT365);
    if declared {
        return Err(DomainError::conflict(ALREADY).into());
    }
    let config = serde_json::to_value(Microsoft365Config {
        directory: application.directory,
        client_id: application.client_id.to_string(),
        issuer: application.issuer,
        graph: MICROSOFT_GRAPH.into(),
    })?;
    let created = create_connector_instance(
        &mut tx,
        NewConnectorInstance {
            tenant_id,
            connector_type: MICROSOFT365,
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
                "Un connecteur porte déjà ce nom, ou l’application Microsoft 365 est déjà déclarée",
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
            "type": MICROSOFT365,
            "directory": application.directory,
            "clientId": application.client_id,
            "enabled": input.enabled,
            "tokenStored": true,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}
