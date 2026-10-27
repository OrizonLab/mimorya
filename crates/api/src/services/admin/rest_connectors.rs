//! REST APIs a company's connector administrators add (ADR-112): the
//! declaration checked as a whole (`mimorya_connectors::rest`), the token
//! sealed in the secret store, never shown nor audited, under the
//! administration lock. Changing, testing and deleting one goes as for an
//! MCP server (`connectors`, `connector_test`). Its operations may be read
//! from an OpenAPI description first (ADR-114).

use mimorya_connectors::outbound::token_problem;
use mimorya_connectors::rest::{Imported, RestConfig, config_problem, import_openapi};
use mimorya_core::DomainError;
use mimorya_db::is_unique_violation;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{NewConnectorInstance, create_connector_instance};
use serde_json::json;
use uuid::Uuid;

use super::connector_config::{operation_names, prefix_taken};
use super::connectors::{ConnectorAdmin, PERMISSION};
use super::{audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::connectors::token_secret_name;

pub struct NewRestConnector {
    pub name: String,
    pub config: RestConfig,
    pub token: Option<String>,
    pub enabled: bool,
}

/// The operations an OpenAPI description offers (ADR-114). Nothing is
/// stored nor called, so nothing is audited.
pub fn read_openapi(access: &AccessContext, document: &str) -> anyhow::Result<Imported> {
    access.require_tenant_wide(PERMISSION)?;
    Ok(import_openapi(document).map_err(DomainError::validation)?)
}

pub async fn create_rest_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    input: NewRestConnector,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let problem = config_problem(&input.config, admin.connectors.network())
        .or_else(|| input.token.as_deref().and_then(token_problem));
    if let Some(problem) = problem {
        return Err(DomainError::validation(problem).into());
    }
    if input.token.is_some() {
        admin.secrets.require()?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    if prefix_taken(&mut tx, "rest", &input.config.prefix).await? {
        return Err(
            DomainError::conflict("Ce préfixe est déjà utilisé par une autre API REST").into(),
        );
    }
    let config = serde_json::to_value(&input.config)?;
    let created = create_connector_instance(
        &mut tx,
        NewConnectorInstance {
            tenant_id,
            connector_type: "rest",
            name: &input.name,
            enabled: input.enabled,
            config: &config,
        },
    )
    .await;
    let id = match created {
        Ok(id) => id,
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict("Un connecteur porte déjà ce nom").into());
        }
        Err(error) => return Err(error.into()),
    };
    if let Some(token) = &input.token {
        admin
            .secrets
            .put(&mut tx, tenant_id, &token_secret_name(id), token)
            .await?;
    }
    append_audit(
        &mut tx,
        audit_entry(access, "connector.created", "connector", id).metadata(json!({
            "name": input.name,
            "type": "rest",
            "prefix": input.config.prefix,
            "url": input.config.url,
            "tokenHeader": input.config.token_header,
            "operations": operation_names(&input.config.operations),
            "enabled": input.enabled,
            "tokenStored": input.token.is_some(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}
